#pragma once

#include <string>

namespace parity {

struct AuditSink {
    std::string last;
    void record(const std::string& value);
};

class BaseHandler {
public:
    virtual ~BaseHandler() = default;
    virtual std::string handle(const std::string& name) = 0;
};

class ConsoleHandler : public BaseHandler {
public:
    explicit ConsoleHandler(AuditSink& sink);
    std::string handle(const std::string& name) override;

private:
    AuditSink& sink_;
};

using HandlerAlias = ConsoleHandler;

std::string format(const std::string& value);
std::string format(int value);

template <typename T>
T choose(T left, T right) {
    return left;
}

#ifdef ENABLE_PARITY_FEATURE
std::string configured_only();
#endif

inline std::string direct_label(const std::string& value) {
    return value;
}

inline std::string expanded_label(const std::string& value) {
    return value;
}

#define PARITY_CALL(function_name, value) parity::function_name(value)

} // namespace parity
