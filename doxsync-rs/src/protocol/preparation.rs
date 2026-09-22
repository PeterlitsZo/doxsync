use std::{collections::BTreeSet, sync::Arc};

use crate::{
    Error, ErrorKind, Result,
    patch::{Action, Path, PathSegment},
    state::{
        InsertPathResult, InsertStringPoolResult, PATH_KEY_BYTES_LIMIT, PATH_PATCH_BYTES_LIMIT,
        PATH_POOL_CAPACITY, PATH_SEGMENTS_LIMIT, ProducerStateSavepoint, ProducerStateTxn,
        STRING_POOL_CAPACITY,
    },
};

use super::{
    consts::{METADATA_PATHS, METADATA_STRINGS},
    layout::{path_definition_len, varuint_len},
    visit_map_keys,
};

/// Definitions in their first-use order. No encoded payloads are allocated.
#[derive(Default)]
pub(crate) struct PreparedPools {
    pub(crate) strings: Vec<(u32, Arc<String>)>,
    pub(crate) paths: Vec<(u32, Arc<Path>)>,
}

#[derive(Clone, Copy)]
#[must_use]
pub(crate) struct PoolSavepoint {
    txn: ProducerStateSavepoint,
    strings_len: usize,
    paths_len: usize,
    string_patch_len: usize,
    path_patch_len: usize,
    string_patch_bytes: usize,
    path_patch_bytes: usize,
    path_definition_bytes: usize,
    actions: usize,
}

/// Prepares one operation sequence without committing its borrowed transaction.
/// Each distinct resource is touched exactly once, even across multiple actions.
pub(crate) struct PoolPreparation<'s> {
    txn: &'s mut ProducerStateTxn,
    string_seen: BTreeSet<Arc<String>>,
    path_seen: BTreeSet<Arc<Path>>,
    strings: Vec<Arc<String>>,
    paths: Vec<Arc<Path>>,
    patches: PreparedPools,
    string_patch_bytes: usize,
    path_patch_bytes: usize,
    path_definition_bytes: usize,
    actions: usize,
    actions_limit: usize,
}

impl<'s> PoolPreparation<'s> {
    pub(crate) fn new(txn: &'s mut ProducerStateTxn, actions_limit: usize) -> Self {
        Self {
            txn,
            string_seen: BTreeSet::new(),
            path_seen: BTreeSet::new(),
            strings: Vec::new(),
            paths: Vec::new(),
            patches: PreparedPools::default(),
            string_patch_bytes: 0,
            path_patch_bytes: 0,
            path_definition_bytes: 0,
            actions: 0,
            actions_limit,
        }
    }

    pub(crate) fn savepoint(&self) -> PoolSavepoint {
        PoolSavepoint {
            txn: self.txn.savepoint(),
            strings_len: self.strings.len(),
            paths_len: self.paths.len(),
            string_patch_len: self.patches.strings.len(),
            path_patch_len: self.patches.paths.len(),
            string_patch_bytes: self.string_patch_bytes,
            path_patch_bytes: self.path_patch_bytes,
            path_definition_bytes: self.path_definition_bytes,
            actions: self.actions,
        }
    }

    pub(crate) fn rollback_to(&mut self, point: PoolSavepoint) {
        assert!(point.strings_len <= self.strings.len());
        assert!(point.paths_len <= self.paths.len());
        assert!(point.string_patch_len <= self.patches.strings.len());
        assert!(point.path_patch_len <= self.patches.paths.len());
        self.txn.rollback_to(point.txn);
        for string in self.strings.drain(point.strings_len..) {
            self.string_seen.remove(&string);
        }
        for path in self.paths.drain(point.paths_len..) {
            self.path_seen.remove(&path);
        }
        self.patches.strings.truncate(point.string_patch_len);
        self.patches.paths.truncate(point.path_patch_len);
        self.string_patch_bytes = point.string_patch_bytes;
        self.path_patch_bytes = point.path_patch_bytes;
        self.path_definition_bytes = point.path_definition_bytes;
        self.actions = point.actions;
    }

    /// Errors restore both the transaction and preparation bookkeeping.
    pub(crate) fn append(&mut self, action: &Action) -> Result<()> {
        let point = self.savepoint();
        let result = self.append_inner(action);
        if result.is_err() {
            self.rollback_to(point);
        }
        result
    }

    fn append_inner(&mut self, action: &Action) -> Result<()> {
        if self.actions >= self.actions_limit {
            return Err(Error::new(ErrorKind::InvalidData, "too many actions"));
        }
        match action {
            Action::Snapshot { value } => {
                visit_map_keys(value, &mut |key| self.prepare_string(key))?;
            }
            Action::Add { path, value } | Action::Replace { path, value } => {
                visit_map_keys(value, &mut |key| self.prepare_string(key))?;
                self.prepare_path(path)?;
            }
            Action::Delete { path } => self.prepare_path(path)?,
            Action::Copy { path, from } => {
                self.prepare_path(path)?;
                self.prepare_path(from)?;
            }
        }
        self.actions += 1;
        Ok(())
    }

    fn prepare_string(&mut self, string: &Arc<String>) -> Result<()> {
        if self.string_seen.contains(string) {
            return Ok(());
        }
        // The previously touched resources must remain resident for all body references.
        if self.strings.len() >= STRING_POOL_CAPACITY {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "too many distinct map keys",
            ));
        }
        self.string_seen.insert(string.clone());
        self.strings.push(string.clone());
        self.txn.hit_string_pool_if_exists(string);
        if self.txn.get_string_key(string).is_none() {
            match self.txn.insert_string_pool(string) {
                InsertStringPoolResult::Inserted { key }
                | InsertStringPoolResult::Replaced { key } => {
                    self.string_patch_bytes +=
                        varuint_len(key as u64) + varuint_len(string.len() as u64) + string.len();
                    self.patches.strings.push((key, string.clone()));
                }
                InsertStringPoolResult::Existing { .. } => unreachable!("new string"),
            }
        }
        Ok(())
    }

    fn prepare_path(&mut self, path: &Path) -> Result<()> {
        if self.path_seen.contains(path) {
            return Ok(());
        }
        if self.paths.len() >= PATH_POOL_CAPACITY {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "too many distinct paths",
            ));
        }
        Self::validate_path(path)?;
        let path = Arc::new(path.clone());
        self.path_seen.insert(path.clone());
        self.paths.push(path.clone());
        self.txn.hit_path_pool_if_exists(&path);
        if self.txn.get_path_key(&path).is_none() {
            let len = path_definition_len(&path);
            // Count actual definitions, including initial hits evicted before their first use.
            self.path_definition_bytes = self
                .path_definition_bytes
                .checked_add(len)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "path patch too large"))?;
            if self.path_definition_bytes > PATH_PATCH_BYTES_LIMIT {
                return Err(Error::new(ErrorKind::InvalidData, "path patch too large"));
            }
            match self.txn.insert_path_pool(&path) {
                InsertPathResult::Inserted { key } | InsertPathResult::Replaced { key } => {
                    self.path_patch_bytes += varuint_len(key as u64) + len;
                    self.patches.paths.push((key, path));
                }
                InsertPathResult::Existing { .. } => unreachable!("new path"),
            }
        }
        Ok(())
    }

    fn validate_path(path: &Path) -> Result<()> {
        if path.segments().len() > PATH_SEGMENTS_LIMIT {
            return Err(Error::new(ErrorKind::InvalidData, "too many path segments"));
        }
        let mut remaining = PATH_KEY_BYTES_LIMIT;
        for segment in path.segments() {
            match segment {
                PathSegment::Key(key) => {
                    remaining = remaining
                        .checked_sub(key.len())
                        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "path keys too large"))?;
                }
                PathSegment::Index(index) => {
                    if u64::try_from(*index).map_or(true, |value| value > u64::MAX >> 2) {
                        return Err(Error::new(ErrorKind::InvalidData, "path index too large"));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn string_key(&self, string: &Arc<String>) -> u32 {
        self.txn
            .get_string_key(string)
            .expect("prepared string missing")
    }

    pub(crate) fn path_key(&self, path: &Path) -> u32 {
        let path = self.path_seen.get(path).expect("path not prepared");
        self.txn.get_path_key(path).expect("prepared path missing")
    }

    pub(crate) fn metadata_len(&self) -> usize {
        let mut len = varuint_len(
            u64::from(!self.patches.strings.is_empty()) + u64::from(!self.patches.paths.is_empty()),
        );
        if !self.patches.strings.is_empty() {
            len += varuint_len(METADATA_STRINGS)
                + varuint_len(self.patches.strings.len() as u64)
                + self.string_patch_bytes;
        }
        if !self.patches.paths.is_empty() {
            len += varuint_len(METADATA_PATHS)
                + varuint_len(self.patches.paths.len() as u64)
                + self.path_patch_bytes;
        }
        len
    }

    /// Leaves prepared state in the borrowed transaction for the caller to commit or roll back.
    pub(crate) fn finish(self) -> PreparedPools {
        self.patches
    }
}
