package example.parity;

public class ConsoleHandler implements Handler {
    @Override
    public String handle(String value) {
        return Names.normalize(value);
    }
}
