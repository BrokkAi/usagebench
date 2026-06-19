import { DEFAULT_TITLE, Greeter, createGreeter, formatName } from "./components.js";

const user = { name: "Ada" };
const greeter = createGreeter();
const direct = new Greeter(DEFAULT_TITLE);
const message = greeter.greet(user);
const label = formatName(user);

export function runApp() {
  return direct.greet({ name: label });
}
