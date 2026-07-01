#include "parity.h"

namespace parity {

void AuditSink::record(const std::string& value) {
    last = value;
}

ConsoleHandler::ConsoleHandler(AuditSink& sink) : sink_(sink) {}

std::string ConsoleHandler::handle(const std::string& name) {
    sink_.record(name);
    return name;
}

std::string format(const std::string& value) {
    return "s:" + value;
}

std::string format(int value) {
    return "i:" + std::to_string(value);
}

} // namespace parity
