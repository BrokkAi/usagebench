package example

type Repository interface {
    Save(value string) string
}

const DefaultPrefix = "job"

var DefaultRepository Repository = &MemoryRepository{}

type MemoryRepository struct {
    Last string
}

func (m *MemoryRepository) Save(value string) string {
    m.Last = value
    return value
}

type Service struct {
    repository Repository
}

func NewService(repository Repository) Service {
    return Service{repository: repository}
}

func (s Service) Execute(name string) string {
    stored := s.repository.Save(name)
    return DefaultPrefix + ":" + stored
}
