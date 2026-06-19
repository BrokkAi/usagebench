DEFAULT_PREFIX = "job"

class Repository:
    def __init__(self):
        self.last = ""

    def save(self, value):
        self.last = value
        return value.strip()

class Service:
    def __init__(self, repository):
        self.repository = repository

    def execute(self, name):
        stored = self.repository.save(name)
        return f"{DEFAULT_PREFIX}:{stored}"

def build_service():
    return Service(Repository())
