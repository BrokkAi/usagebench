exports.accepts = function accepts(contentType) {
  return contentType === "application/json" || contentType === "text/html";
};
