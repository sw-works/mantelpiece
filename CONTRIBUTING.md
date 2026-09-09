# Contributing

MemoryLane is a hackathon entry, but pull requests and issues are welcome.

## Before you open a pull request

1. Work against `main`.
2. Open an issue first for anything larger than a fix, so the change can be discussed before the work.
3. Run `make lint` and `make test`. Both run without AWS credentials.
4. Keep the change focused. Reformatting unrelated files makes review harder.
5. Use conventional commit messages (`feat:`, `fix:`, `docs:`, `chore:`), one concern per commit.

## Where things live

See the layout in [README.md](README.md). The TV app must stay fetch-only with no native
modules so one codebase runs on Vega OS and Fire OS; platform differences go in
`.kepler.ts` files next to the default implementation.

## Reporting a security issue

Do not open a public issue. Email the maintainer listed in the repository profile.

## License

By contributing you agree that your contribution is licensed under the MIT license in
[LICENSE](LICENSE).
