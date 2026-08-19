import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const site = process.env.PUBLIC_DOCS_SITE ?? 'https://brokkai.github.io';
const productionBase = process.env.PUBLIC_DOCS_BASE ?? '/usagebench';
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
            { label: 'Historical v0.2.0 result', slug: 'results/evaluation-real-project-v1' },
            {
              label: 'Historical v0.2.0 cases',
              slug: 'results/evaluation-real-project-v1-case-comparison',
            },
            { label: 'Historical development result', slug: 'results/development-2026-07-24' },
            { label: 'Historical development cases', slug: 'results/development-case-comparison' },
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
