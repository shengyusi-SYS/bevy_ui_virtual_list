# Development

[简体中文](development.zh-CN.md) · [Usage](../README.md)

## Set up

Install Rust 1.95 or newer with Cargo and rustfmt. The crate uses official Bevy
0.19.1. Run commands from this repository's root.

On Ubuntu, install the desktop build libraries:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libudev-dev libwayland-dev libxkbcommon-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libxinerama-dev
```

On Windows, install the Visual Studio C++ build tools and Windows SDK. On macOS,
install the Xcode command-line tools with `xcode-select --install`. Running the
example uses a desktop session and a graphics driver supported by Bevy.

## Build and check

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo test --lib
cargo doc --no-deps
cargo run --example basic
```

Tests exercise geometry, ECS systems and layout in memory. Use the windowed
example to check input and appearance. Generated API docs are in
`target/doc/bevy_ui_virtual_list/index.html`. CI runs formatting, compilation, unit tests and
API documentation on Linux with stable Rust.

## Source map

| File | Responsibility |
| --- | --- |
| `src/lib.rs` | Public configuration, plugin and visible-range updates |
| `src/geometry.rs` | Content height, row position, rendered offset and outer-viewport intersection |
| `src/managed.rs` | Content node, row allocation, placement and interaction cleanup |
| `src/navigation.rs` | Index requests, cancellation and motion-adapter events |
| `src/tests.rs`, `src/managed_tests.rs` | Geometry, lifecycle and layout regression tests |
| `examples/basic.rs` | Runnable interaction demo |

Update runs extent preparation, pending navigation, visible-range calculation
and row allocation in `PooledListRangeUpdate`. Application Build and Bind systems
follow in that order. PostUpdate checks the layout and requests another frame
when a newly available extent or viewport needs it.

For changes to row allocation or navigation, run the example in both viewport
modes. Scroll, resize, replace data at the same length, shorten it and jump near
the end. Check that recycled text and actions match the new index. For delayed
content, keep item identity checks in the caller's binding code.

## Make a change

Start from the affected public component or event and follow it into the system
that consumes it. Add a focused regression test for changed behavior. Keep the
library's data expressed as Bevy entities, components and geometry; application
content enters through the public events and binding points.

Update the English and Chinese README together when a public API or setup step
changes. Keep development instructions in this guide and API details beside the
public types. Run the checks above before opening a pull request, and describe
the user-visible result and how you checked it.

## Release

Update the package version and the Git tag in both README installation examples.
Run the checks, commit the release, then tag that commit as `vX.Y.Z` and push the
commit and tag. Create a GitHub release with concise English and Chinese notes.
Keep the bundled license files with the source.
