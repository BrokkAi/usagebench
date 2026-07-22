using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.Text;

namespace UsageBench.Generators;

[Generator]
public sealed class GeneratedNameGenerator : ISourceGenerator
{
    public void Initialize(GeneratorInitializationContext context)
    {
    }

    public void Execute(GeneratorExecutionContext context)
    {
        context.AddSource(
            "GeneratedConsumer.GeneratedName.g.cs",
            SourceText.From(
                """
                namespace Example.Generated;

                public partial class GeneratedConsumer
                {
                    public partial string GeneratedName()
                    {
                        return "generated";
                    }
                }
                """,
                Encoding.UTF8));
    }
}
