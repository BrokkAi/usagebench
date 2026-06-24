const request = require("./commonjs-request");

const prefersJson = request.accepts("application/json");

function checkHtml() {
  return request.accepts("text/html") && prefersJson;
}

module.exports = { checkHtml };
