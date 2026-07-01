package example.parity;

public class Runner {
    public String run() {
        Handler handler = new ConsoleHandler();
        ConsoleHandler direct = new ConsoleHandler();
        return handler.handle(" Ada ") + ":" + direct.handle(" Grace ");
    }

    public Handler makeAnonymous() {
        return new Handler() {
            @Override
            public String handle(String value) {
                return Names.DEFAULT_PREFIX + value;
            }
        };
    }

    public String runAnonymous() {
        return makeAnonymous().handle(" Lin ");
    }
}
