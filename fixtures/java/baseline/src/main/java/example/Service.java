package example;

public class Service {
    public static final String DEFAULT_PREFIX = "job";

    private final Repository repository;

    public Service(Repository repository) {
        this.repository = repository;
    }

    public String execute(String name) {
        String stored = repository.save(name);
        return DEFAULT_PREFIX + ":" + stored;
    }

    public static class Repository {
        public String save(String value) {
            return value.trim();
        }
    }
}
