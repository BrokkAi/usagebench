package precision;

class Worker {
    void execute() {}
}

record Entry(String value) {}

enum Mode { FAST }

class Consumer {
    void run() {
        Runnable task = () -> new Worker().execute();
        task.run();
        Entry entry = new Entry("value");
        Mode mode = Mode.FAST;
        Helpers.log();
    }
}
