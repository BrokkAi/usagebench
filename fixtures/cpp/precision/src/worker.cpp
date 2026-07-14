#include "worker.h"

namespace precision {

void Worker::execute() {}

int select(int value) { return value; }
int select(const char* value) { return value[0]; }

} // namespace precision
