import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const site = process.env.PUBLIC_DOCS_SITE ?? 'https://usagebench.brokk.ai';
const productionBase = process.env.PUBLIC_DOCS_BASE ?? '/';
const isDev = process.argv.includes('dev');

export default defineConfig({
  site,
  base: isDev ? '/' : productionBase,
  integrations: [
    starlight({
      title: 'UsageBench',
      description: 'LSP-parity and recurring regression evidence for Bifrost usage analysis.',
      customCss: ['./src/styles/usagebench.css'],
      favicon: '/favicon.svg',
      editLink: {
        baseUrl: 'https://github.com/BrokkAi/usagebench/edit/main/docs/',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/BrokkAi/usagebench',
        },
      ],
      sidebar: [
        {
          label: 'About the benchmark',
          items: [
            { label: 'Overview', slug: 'overview' },
            { label: 'Comparison methodology', slug: 'methodology' },
            { label: 'Human ground-truth audit', slug: 'ground-truth-review' },
            { label: 'Reproduce the comparison', slug: 'reproduce' },
          ],
        },
        {
          label: 'Results and findings',
          items: [
            { label: 'Evidence map (generated)', slug: 'results/evidence' },
            { label: 'Current evaluation result', slug: 'results' },
            { label: 'Evaluation case comparison', slug: 'results/case-comparison' },
          ],
        },
        {
          // Superseded snapshots stay reachable and stay collapsed. They are
          // older analyzer versions against older corpora, and leaving them
          // beside the current result invited reading them as current.
          label: 'Superseded snapshots',
          collapsed: true,
          items: [
            { label: 'v0.2.0 evaluation result', slug: 'results/evaluation-real-project-v1' },
            {
              label: 'v0.2.0 evaluation cases',
              slug: 'results/evaluation-real-project-v1-case-comparison',
            },
            { label: '24 July development result', slug: 'results/development-2026-07-24' },
            { label: '24 July development cases', slug: 'results/development-case-comparison' },
          ],
        },
        {
          label: 'Development findings by language',
          items: [
            { label: 'C++ and clangd', slug: 'languages/cpp' },
            { label: 'C# and Roslyn', slug: 'languages/csharp' },
            { label: 'Go and gopls', slug: 'languages/go' },
            { label: 'Java and JDT LS', slug: 'languages/java' },
            { label: 'JavaScript and TypeScript', slug: 'languages/javascript-typescript' },
            { label: 'PHP and Intelephense', slug: 'languages/php' },
            { label: 'Python and Pyright', slug: 'languages/python' },
            { label: 'Ruby and Ruby LSP', slug: 'languages/ruby' },
            { label: 'Rust and rust-analyzer', slug: 'languages/rust' },
            { label: 'Scala and Metals', slug: 'languages/scala' },
          ],
        },
      ],
    }),
  ],
});
