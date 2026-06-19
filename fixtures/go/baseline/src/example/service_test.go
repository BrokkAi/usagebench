package example

func ExampleService() {
    repository := &MemoryRepository{}
    service := NewService(repository)
    result := service.Execute("Ada")
    _ = DefaultRepository
    _ = DefaultPrefix + result
    repository.Save("Grace")
    _ = repository.Last
}
