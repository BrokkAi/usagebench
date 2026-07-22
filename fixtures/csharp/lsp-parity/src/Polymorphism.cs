namespace Example.Parity;

public sealed class BufferHandler : IHandler
{
    public string Handle(string name)
    {
        return name;
    }
}

public static partial class HandlerFactory
{
    public static IHandler CreateAmbiguous(bool useConsole)
    {
        return useConsole
            ? new ConsoleHandler(new ConsoleSink())
            : new BufferHandler();
    }
}

public static class PolymorphismConsumer
{
    public static string Run(bool useConsole)
    {
        IHandler handler = HandlerFactory.CreateAmbiguous(useConsole);
        var viaInterface = handler.Handle("Ada");

        var console = new ConsoleHandler(new ConsoleSink());
        var viaConsole = console.Handle("Ben");

        var buffer = new BufferHandler();
        var viaBuffer = buffer.Handle("Cal");

        return viaInterface + viaConsole + viaBuffer;
    }
}
