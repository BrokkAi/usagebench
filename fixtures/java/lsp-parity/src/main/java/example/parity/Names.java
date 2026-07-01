package example.parity;

public final class Names {
    private Names() {
    }

    public static String normalize(String value) {
        return value.trim();
    }

    public static final String DEFAULT_PREFIX = "job";
}
