# Rustpackages — running a mirror & packaging apps

- **Mirror (Docker):** see `docker/docker-compose.yml` — serves a published
  repo (`index.toml` + `pkgs/*.mpk`) via nginx on port 8080.
  Publish a repo with `docker/publish.sh <packages-dir> <output-dir>`.
- **Packaging guide:** see [docs/PACKAGING.md](docs/PACKAGING.md) for the
  `.mpk` format, `pkg.toml` manifest, build steps, and publishing flow.
