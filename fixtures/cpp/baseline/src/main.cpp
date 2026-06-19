#include "service.h"

namespace example {

std::string run_demo() {
    Repository repository;
    auto service = build_service(repository);
    auto result = service.execute(" Ada ");
    return std::string(DefaultPrefix) + result + repository.last;
}

} // namespace example
