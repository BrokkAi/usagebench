import { createWidget } from "./api";

function run(createWidget: () => unknown) {
  createWidget();
}

run(() => "local");
