function buildTask(label) {
  return new Task(label);
}

class Task {
  constructor(label) {
    this.label = label;
  }

  finish() {
    return helpers.formatTask(this);
  }
}

const helpers = {
  formatTask(task) {
    return task.label;
  },
};

exports.buildTask = buildTask;
exports.Task = Task;
exports.helpers = helpers;
