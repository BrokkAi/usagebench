export const DEFAULT_TITLE = "Welcome";

export type User = {
  name: string;
};

export function formatName(user: User): string {
  return user.name.trim();
}

export default class Greeter {
  private title: string;

  constructor(title = DEFAULT_TITLE) {
    this.title = title;
  }

  greet(user: User): string {
    return `${this.title}, ${formatName(user)}`;
  }
}

export function WelcomeCard({ user }: { user: User }) {
  const greeter = new Greeter();
  return <section>{greeter.greet(user)}</section>;
}
