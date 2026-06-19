export const DEFAULT_TITLE = "Welcome";

export class Greeter {
  constructor(title = DEFAULT_TITLE) {
    this.title = title;
  }

  greet(user) {
    return `${this.title}, ${formatName(user)}`;
  }
}

export function formatName(user) {
  return user.name.trim();
}

export function createGreeter() {
  return new Greeter(DEFAULT_TITLE);
}
