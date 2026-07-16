# UsageBench docs

The public documentation site uses Astro Starlight, matching the documentation
stack used by Bifrost. Source pages live under `src/content/docs/`; the older
implementation plans and runner notes remain at this directory's top level.

```bash
npm ci
npm run check
npm run build
npm run dev
```

Production builds use `https://brokkai.github.io/usagebench` as their default
site and base path. Local development uses `/`.
