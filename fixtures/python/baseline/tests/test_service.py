from example import DEFAULT_PREFIX, Service, build_service
from example.service import Repository

def test_service_execution():
    service = build_service()
    result = service.execute(" Ada ")
    assert result == DEFAULT_PREFIX + ":Ada"

def test_repository_attribute():
    repository = Repository()
    repository.save("Grace")
    assert repository.last == "Grace"

def test_dynamic_lookup():
    service = build_service()
    method_name = "execute"
    getattr(service, method_name)("Linus")
