#include "worker.h"

void consume() {
  precision::Worker worker;
  worker.execute();
  auto name = precision::select("name");
}
// worker.execute();
const char* label = "worker.execute";
