import Greeter, { DEFAULT_TITLE, WelcomeCard, formatName, type User } from "./components";

const user: User = { name: "Ada" };
const greeter = new Greeter(DEFAULT_TITLE);
const message = greeter.greet(user);
const label = formatName(user);

export function App() {
  return <WelcomeCard user={user} />;
}
