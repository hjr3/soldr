build: build-ui build-core

build-ui:
  cd packages/ui && npm run build
  cp -r packages/ui/dist/* crates/ui/static/

build-core:
  cargo build

install:
  cd packages/ui && [ "${CI:-false}" == "true" ] && npm ci || npm i
