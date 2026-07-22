const { buildTask, Task, helpers } = require("./library");

const directTask = buildTask("direct");
const constructed = new Task("class");
helpers.formatTask(directTask);
constructed.finish();
constructed["finish"]();

const methodName = "finish";
constructed[methodName]();
