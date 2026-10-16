import { Decimal } from "@doxsync/core";

type Task = { id: string; title: string; done: boolean };

const taskTemplates: readonly Task[] = [
  { id: "task-1", title: "Load the document", done: true },
  { id: "task-2", title: "Send an update", done: false },
  { id: "task-3", title: "Inspect the message", done: false },
  { id: "task-4", title: "Compare the documents", done: false },
  { id: "task-5", title: "Toggle a task", done: false },
  { id: "task-6", title: "Reorder the tasks", done: false },
  { id: "task-7", title: "Inspect the string pool", done: false },
  { id: "task-8", title: "Replay the changes", done: false },
];

export type DemoDocument = {
  title: string;
  visitors: bigint;
  price: Decimal;
  online: boolean;
  settings: { theme: string; volume: number };
  tags: string[];
  tasks: Task[];
  note?: string;
};
export type Mutation = (document: DemoDocument) => string;

export const initialDocument = (): DemoDocument => ({
  title: "doxsync playground",
  visitors: 12n,
  price: new Decimal("12.34"),
  online: true,
  settings: { theme: "light", volume: 60 },
  tags: ["rust", "wasm"],
  tasks: taskTemplates.slice(0, 6).map((task) => ({ ...task })),
});
const pick = <T,>(items: readonly T[]): T => items[Math.floor(Math.random() * items.length)];
const number = (min: number, max: number) => min + Math.floor(Math.random() * (max - min + 1));

export const mutations: readonly Mutation[] = [
  (doc) => { doc.price = doc.price.plus("0.01"); return `price → ${doc.price.toFixed(2)}`; },
  (doc) => { const delta = BigInt(number(1, 5)); doc.visitors += delta; return `visitors increased by ${delta}`; },
  (doc) => { doc.online = !doc.online; return `online → ${doc.online}`; },
  (doc) => { doc.settings.theme = pick(["light", "dark", "system"].filter((value) => value !== doc.settings.theme)); return `settings.theme → ${doc.settings.theme}`; },
  (doc) => { doc.settings.volume = (doc.settings.volume + number(1, 4) * 10) % 110; return `settings.volume → ${doc.settings.volume}`; },
  (doc) => { doc.title = pick(["doxsync playground", "Hello from Rust", "Hello, WebAssembly", "Small changes, in sync"].filter((value) => value !== doc.title)); return `title → ${doc.title}`; },
  (doc) => {
    if (doc.tags.length >= 5 || (doc.tags.length > 1 && Math.random() < .5)) {
      const [removed] = doc.tags.splice(number(0, doc.tags.length - 1), 1);
      return `Removed ${removed} from tags`;
    }
    const tag = pick(["rust", "wasm", "browser", "sync", "binary", "js"].filter((value) => !doc.tags.includes(value)));
    doc.tags.push(tag);
    return `Added ${tag} to tags`;
  },
  (doc) => {
    const index = number(0, doc.tasks.length - 1);
    const task = doc.tasks[index];
    task.done = !task.done;
    return `tasks[${index}].done → ${task.done} (${task.id})`;
  },
  (doc) => {
    if (doc.tasks.length === taskTemplates.length || (doc.tasks.length > 1 && Math.random() < .5)) {
      const index = number(0, doc.tasks.length - 1);
      const [removed] = doc.tasks.splice(index, 1);
      return `Removed ${removed.id} from tasks[${index}]`;
    }
    const task = { ...pick(taskTemplates.filter((item) => !doc.tasks.some((task) => task.id === item.id))) };
    const index = number(0, doc.tasks.length);
    doc.tasks.splice(index, 0, task);
    return `Inserted ${task.id} at tasks[${index}]`;
  },
  (doc) => {
    if (doc.tasks.length < 2) return "Task order unchanged";
    const from = number(0, doc.tasks.length - 1);
    const to = (from + number(1, doc.tasks.length - 1)) % doc.tasks.length;
    const [task] = doc.tasks.splice(from, 1);
    doc.tasks.splice(to, 0, task);
    return `Moved ${task.id} from tasks[${from}] to tasks[${to}]`;
  },
  (doc) => {
    if (Object.hasOwn(doc, "note")) { delete doc.note; return "Removed the note field"; }
    doc.note = "This message is on its way";
    return "Added the note field";
  },
  () => "Document unchanged",
];

export function mutateDocument(document: DemoDocument) {
  const next = {
    ...document,
    settings: { ...document.settings },
    tags: [...document.tags],
    tasks: document.tasks.map((task) => ({ ...task })),
    price: new Decimal(document.price),
  };
  return { document: next, summary: pick(mutations)(next) };
}
