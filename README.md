# wlstreamer

Wrapper around `wf-recorder` and `ffmpeg` that automatically switches the screen being recorded based on current window focus. Works with `sway`.

## Installation

There's an AUR package called `wlstreamer-git`, alternatively you can build and install manually:

1. Make sure you're running sway
2. Install `wf-recorder`, `v4l2loopback` and `ffmpeg`
3. Load the `v4l2loopback` kernel module
4. Install the rust toolchain
5. Clone this repo
6. `cargo install --path . --root ~/.local`



## Usage

See `wlstreamer --help`. If there are no screens available for streaming, a black screen will be shown instead.

---

Usage: wlstreamer [options]
Wrapper around wf-recorder and ffmpeg that automatically switches the screen being recorded based on current window focus

Options:
  --not-ws <ws-num>         Do not show this workspace. Can be used multiple times. Example: 3
  --not-screen <screen>     Do not show this screen. Can be used multiple times. Example: HDMI-A-1
  -d|--devices-from <id>    Use video devices starting at $id. Defaults to 0. /dev/video$id will be used as output. See DIFFERENT RESOLUTIONS below.
  -v|--version              Display version and exit
  --verbose                 Verbose logging

If there are no screens available for streaming, a black screen will be shown instead.

DIFFERENT RESOLUTIONS

When running outputs with different resolutions, the resulting stream will be the smallest possible resolution that can fit all output resolutions.
For example, two outputs, one 1600x1200, another 1920x1080, will result in an output stream of 1920x1200. Any remaining space will be padded black.
Another example, two outputs, one 640x480, another 1920x1080, will result in an output stream of 1920x1080. Space will only be padded black on the smaller screen.

To support this behaviour, wlstreamer needs access to a v4l2loopback device for each resolution, included the combined upscaled one if applicable. For the first example above, this would mean you would need 3 devices. For the second, you'd need two. If all your outputs have the same resolution, you only need an output device.

The --devices-from or -d option specifies at which device index it is okay to start using loopback devices. For example, if you specify -d 3, and you need 2 capture devices, /dev/video3 and /dev/video4 will be used by wlstreamer, with /dev/video3 being the output you want to use in other applications.

DYNAMICALLY CHANGING RESOLUTIONS

As long as you have enough v4l2loopback devices available for new resolutions, it should be fine to change resolutions on an output.
However, if your resolution is either wider or taller than the output resolution, this will result in failures, since dynamically changing the v4l2loopback device resolution is not possible.
