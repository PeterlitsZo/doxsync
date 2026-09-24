# AGENTS.md

## Behavior

- Use English for all project content, including UI text, documentation, comments,
  and example data.
- DO NOT add unit tests until the user asks you to do.

## Protocol Versions

- If the latest protocol version in `WIRE.md` lists only `[UNRELEASED]`,
  apply protocol changes to that version instead of creating a new version.

## When You Need to Commit

WHEN THE USER ASKS YOU TO COMMIT, you MUST follow this section.

### Before Committing

Before committing, you MUST:

- Explicitly decide whether `CHANGELOG.md` needs an update. Keep it concise.
  Add only user-visible changes such as new features, behavioral changes,
  compatibility-impacting fixes, or important bug fixes. Do not add pure
  internal refactors that do not affect functionality. If you update
  `CHANGELOG.md`, you MUST ask the user to confirm that it looks correct before
  committing, and you MUST stage it explicitly.
- Update `WIRE.md`.

### Git Commit Message

Git commit messages use the following format:

```
<type>: <subject>
```

Here, `<type>` describes the nature of the change, such as `feat`, `fix`,
`docs`, `chore`, or `refactor`.

The `<subject>` should be a short description of the change in English. Keep it
concise and clear. You need to review all changed files and summarize the most
important change (for example, if the project includes both a refactor and a
documentation update, the subject should describe the refactor rather than the
documentation update). The subject MUST end with an English full stop, and its
first letter MUST be capitalized.
