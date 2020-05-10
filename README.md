# stream-screen

Wrapper around `wf-recorder` and `ffmpeg` that automatically switches the screen being recorded based on current window focus. Works with `sway`.

## Installation

1. Make sure you're running sway
2. Install `wf-recorder` and `ffmpeg`
3. Install the rust toolchain
4. Clone this repo
5. `cargo install --path . --root ~/.local`

## Usage

See `--help`. If there are no screens available for streaming, a black screen will be shown instead.
