class Client {
  request() {}
}

function create() {
  return new Client();
}

module.exports = { Client, create };
