#include "service.h"

namespace example {

std::string Repository::save(const std::string& value) {
    last = value;
    return last;
}

Service::Service(Repository& repository) : repository_(repository) {}

std::string Service::execute(const std::string& name) {
    auto stored = repository_.save(name);
    return std::string(DefaultPrefix) + ":" + stored;
}

Service build_service(Repository& repository) {
    return Service(repository);
}

} // namespace example
