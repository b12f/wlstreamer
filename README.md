# stream-screen

Wrapper around `wf-recorder` and `ffmpeg` that automatically switches the screen being recorded based on current window focus. Works with `sway`.

## Installation

1. Make sure you're running sway
2. Install `wf-recorder`, `v4l2loopback` and `ffmpeg`
3. Load the `v4l2loopback` kernel module
4. Install the rust toolchain
5. Clone this repo
6. `cargo install --path . --root ~/.local`

## Usage

See `--help`. If there are no screens available for streaming, a black screen will be shown instead.
