package example.parity;

import static example.parity.Names.normalize;

public class StaticImportUser {
    public String display(String raw) {
        return normalize(raw);
    }
}
