set shell := ["bash", "-lc"]

default:
  @just --list

fmt:
  cargo fmt

test:
  cargo test

build:
  cargo build

build-servo:
  env BRAZEN_SERVO_SOURCE=vendor/servo cargo build --features servo

init-servo:
  git submodule update --init --recursive vendor/servo
