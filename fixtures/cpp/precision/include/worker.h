#pragma once

namespace precision {

struct Worker {
  void execute();
};

int select(int value);
int select(const char* value);

} // namespace precision
