using System;

namespace Precision;

[Tracked]
public sealed class Registered {}

public sealed class TrackedAttribute : Attribute {}

public static class Extensions {
    public static T Echo<T>(this T value) => value;
}

public static class Labels {
    public static string Create() => "ready";
}

public static class Consumer {
    public static Registered Run() => new Registered().Echo();
    public static string Label() => Labels.Create();
}
