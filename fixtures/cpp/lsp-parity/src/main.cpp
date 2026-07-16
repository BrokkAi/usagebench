#include "parity.h"

namespace app {

std::string run() {
    parity::AuditSink sink;
    parity::HandlerAlias handler(sink);
    parity::BaseHandler& base = handler;
    auto first = base.handle("Ada");
    auto second = handler.handle("Ben");
    auto formatted = parity::format(first);
    auto number = parity::format(7);
    auto chosen = parity::choose<std::string>(formatted, sink.last);
    auto direct = parity::direct_label("direct");
    auto expanded = PARITY_CALL(expanded_label, std::string("expanded"));
    return chosen + second + number + direct + expanded;
}

} // namespace app
