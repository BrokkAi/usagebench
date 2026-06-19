#pragma once

#include <string>

namespace example {

struct Repository {
    std::string last;
    std::string save(const std::string& value);
};

class Service {
public:
    explicit Service(Repository& repository);
    std::string execute(const std::string& name);

private:
    Repository& repository_;
};

inline constexpr const char* DefaultPrefix = "job";

Service build_service(Repository& repository);

} // namespace example
