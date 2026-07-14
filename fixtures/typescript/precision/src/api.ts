export interface Widget {
  title: string;
}

export function createWidget(): Widget {
  return { title: "ready" };
}
