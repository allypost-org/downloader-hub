# docker buildx bake definition for the downloader-hub images.
#
# All three targets share the `chef`/`planner`/`deps` stages of `.docker/Dockerfile`,
# so BuildKit compiles the workspace dependency graph exactly once per invocation.
#
# Usage:
#   docker buildx bake --print            # show the resolved build plan
#   docker buildx bake                    # build all (default group) locally
#   docker buildx bake bot                # build a single target
#
# Override versions / tags via env vars (bake reads these by name):
#   SHA=abc123 REGISTRY=allypost docker buildx bake --print
#
# CI injects the S3 cache config via `docker/bake-action`'s `set` input so that
# AWS credentials never need to live in this file.

variable "REGISTRY" {
  default = "allypost"
}

variable "SHA" {
  default = "dev"
}

variable "RUST_VERSION" {
  default = "1.96"
}

variable "UPX_VERSION" {
  default = "5.2.0"
}

variable "YT_DLP_VERSION" {
  default = "latest"
}

group "default" {
  targets = ["admin", "bot", "central", "worker"]
}

# Shared defaults, not a buildable target on its own.
target "common" {
  context    = "."
  dockerfile = ".docker/Dockerfile"
  pull       = true
  platforms  = ["linux/amd64"]
  args = {
    RUST_VERSION   = RUST_VERSION
    UPX_VERSION    = UPX_VERSION
    YT_DLP_VERSION = YT_DLP_VERSION
  }
}

target "bot" {
  inherits = ["common"]
  target   = "bot"
  args     = { BINARY_NAME = "downloader-bot" }
  tags = [
    "${REGISTRY}/downloader-bot:latest",
    "${REGISTRY}/downloader-bot:${SHA}",
  ]
}

target "central" {
  inherits = ["common"]
  target   = "central"
  args     = { BINARY_NAME = "downloader-central" }
  tags = [
    "${REGISTRY}/downloader-central:latest",
    "${REGISTRY}/downloader-central:${SHA}",
  ]
}

target "worker" {
  inherits = ["common"]
  target   = "worker"
  args     = { BINARY_NAME = "downloader-worker" }
  tags = [
    "${REGISTRY}/downloader-worker:latest",
    "${REGISTRY}/downloader-worker:${SHA}",
  ]
}

target "admin" {
  inherits = ["common"]
  target   = "admin"
  args     = { BINARY_NAME = "downloader-admin" }
  tags = [
    "${REGISTRY}/downloader-admin:latest",
    "${REGISTRY}/downloader-admin:${SHA}",
  ]
}
