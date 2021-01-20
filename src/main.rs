use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Error};
use std::process::{Child, Command, Stdio};
use std::{thread, time};

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SwayScreenRect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SwayWorkspace {
    name: String,
    focus: Vec<usize>,
    output: String,
    focused: bool,
    rect: SwayScreenRect,
    visible: bool,
    num: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SwayOutputMode {
    width: usize,
    height: usize,
    refresh: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SwayOutput {
    name: String,
    rect: SwayScreenRect,
    current_mode: SwayOutputMode,
}

#[derive(Copy, Clone, Hash, Eq, Debug)]
struct Resolution {
    height: usize,
    width: usize,
}

#[derive(Debug)]
struct Config {
    current_output: String,
    devices_from: usize,
    last_device_index: usize,
    screen_blacklist: Vec<String>,
    workspace_blacklist: Vec<usize>,
    verbose: bool,
    resolutions: Vec<Resolution>,
    outputs: HashMap<Resolution, usize>,
}

impl PartialEq<Resolution> for Resolution {
    fn eq(&self, other: &Resolution) -> bool {
        self.width == other.width && self.height == other.height
    }
}

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn help() {
    println!("Usage: wlstreamer [options]");
    println!("Wrapper around wf-recorder and ffmpeg that automatically switches the screen being recorded based on current window focus");
    println!("");
    println!("Options:");
    println!("  --not-ws <ws-num>         Do not show this workspace. Can be used multiple times. Example: 3");
    println!("  --not-screen <screen>     Do not show this screen. Can be used multiple times. Example: HDMI-A-1");
    println!("  -d|--devices-from <id>    Use video devices starting at $id. Defaults to 0. /dev/video$id will be used as output. See DIFFERENT RESOLUTIONS below.");
    println!("  -v|--version              Display version and exit");
    println!("  --verbose                 Verbose logging");
    println!("");
    println!(
        "If there are no screens available for streaming, a black screen will be shown instead."
    );
    println!("");
    println!("DIFFERENT RESOLUTIONS");
    println!("");
    println!("When running outputs with different resolutions, the resulting stream will be the smallest possible resolution that can fit all output resolutions.");
    println!("For example, two outputs, one 1600x1200, another 1920x1080, will result in an output stream of 1920x1200. Any remaining space will be padded black.");
    println!("Another example, two outputs, one 640x480, another 1920x1080, will result in an output stream of 1920x1080. Space will only be padded black on the smaller screen.");
    println!("");
    println!("To support this behaviour, wlstreamer needs access to a v4l2loopback device for each resolution, included the combined upscaled one if applicable. For the first example above, this would mean you would need 3 devices. For the second, you'd need two. If all your outputs have the same resolution, you only need an output device.");
    println!("");
    println!("The --devices-from or -d option specifies at which device index it is okay to start using loopback devices. For example, if you specify -d 3, and you need 2 capture devices, /dev/video3 and /dev/video4 will be used by wlstreamer, with /dev/video3 being the output you want to use in other applications.");
    println!("");
    println!("DYNAMICALLY CHANGING RESOLUTIONS");
    println!("");
    println!("As long as you have enough v4l2loopback devices available for new resolutions, it should be fine to change resolutions on an output.");
    println!("However, if your resolution is either wider or taller than the output resolution, this will result in failures, since dynamically changing the v4l2loopback device resolution is not possible.");

    std::process::exit(0);
}

fn stream_black(config: &mut Config) -> Result<Vec<Box<Child>>, Error> {
    let cmd = Command::new("ffmpeg")
        .args(&[
            "-i",
            format!(
                "color=c=black:s={}x{}:r=25/1",
                config.resolutions[0].width, config.resolutions[0].height
            )
            .as_str(),
            "-vcodec",
            "rawvideo",
            "-pix_fmt",
            "yuyv422",
            "-f",
            "v4l2",
            format!("/dev/video{}", config.devices_from).as_str(),
        ])
        .stdin(Stdio::piped())
        .stdout(if config.verbose {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .stderr(if config.verbose {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .spawn()?;

    config.current_output = "".to_string();

    return Ok(vec![Box::new(cmd)]);
}

fn record_screen(config: &mut Config, output: SwayOutput) -> Result<Vec<Box<Child>>, Error> {
    let resolution = Resolution {
        height: output.current_mode.height,
        width: output.current_mode.width,
    };

    let device_number = match config.outputs.get(&resolution) {
        Some(device_number) => *device_number,
        None => {
            config.last_device_index += 1;
            config.outputs.insert(resolution, config.last_device_index);
            config.last_device_index
        }
    };

    if config.verbose {
        println!("Using device number {}", device_number);
    }

    let output_str = format!("--file=/dev/video{}", device_number);
    let screen_str = format!("-o{}", output.name.as_str());
    let recorder = Command::new("wf-recorder")
        .args(&[
            "--muxer=v4l2",
            "--codec=rawvideo",
            "--pixel-format=yuyv422",
            screen_str.as_str(),
            output_str.as_str(),
        ])
        .stdin(Stdio::piped())
        .stdout(if config.verbose {
            Stdio::inherit()
        } else {
            Stdio::piped()
        })
        .stderr(if config.verbose {
            Stdio::inherit()
        } else {
            Stdio::piped()
        })
        .spawn()?;

    config.current_output = output.name.as_str().to_string();

    let mut processes = vec![Box::new(recorder)];

    if device_number != config.devices_from {
        if config.verbose {
            println!("Does not have the maximum combined resolution, filtering through ffmpeg");
        }

        // TODO: This is slow, ugly, and prone to failure. ffmpeg will fail if wf-recorder isn't
        // writing yet, however I'm not sure how to get an exact timing of when it's okay to start
        // reading from the device.
        thread::sleep(time::Duration::from_millis(100));

        let upscaler = Command::new("ffmpeg")
            .args(&[
                "-i",
                format!("/dev/video{}", device_number).as_str(),
                "-vcodec",
                "rawvideo",
                "-pix_fmt",
                "yuyv422",
                "-f",
                "v4l2",
                "-vf",
                format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1",
                    config.resolutions[0].width, config.resolutions[0].height,
                    config.resolutions[0].width, config.resolutions[0].height).as_str(),
                format!("/dev/video{}", config.devices_from).as_str(),
            ])
            .stdin(Stdio::piped())
            .stdout(if config.verbose {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .stderr(if config.verbose {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .spawn()?;

        processes.push(Box::new(upscaler));
    }

    return Ok(processes);
}

fn get_outputs(config: &mut Config) -> Vec<SwayOutput> {
    let command = "swaymsg -t get_outputs";
    let output = Command::new("sh")
        .args(&["-c", command])
        .output()
        .expect("Error running swaymsg");

    let stdout_string = String::from_utf8(output.stdout).expect("Invalid UTF-8 from get_outputs");
    let outputs: Vec<SwayOutput> =
        serde_json::from_str(stdout_string.as_str()).expect("Invalid json from get_outputs");

    if config.verbose {
        println!("Found outputs");
        for elem in outputs.iter() {
            println!("{:?}", elem);
        }
    }

    return outputs;
}

fn get_output(config: &mut Config, screen: &str) -> SwayOutput {
    let outputs = get_outputs(config);
    let output = match outputs.iter().find(|o| o.name == screen) {
        Some(o) => o.to_owned(),
        None => panic!("Could not find output"),
    };

    return output.clone();
}

fn get_resolutions(config: &mut Config) -> Vec<Resolution> {
    let outputs = get_outputs(config);
    let mut resolutions: Vec<Resolution> = outputs
        .iter()
        .map(|o| Resolution {
            height: o.current_mode.height,
            width: o.current_mode.width,
        })
        .unique()
        .collect_vec();

    if config.verbose {
        println!("{:?}", resolutions);
    }

    let combined_resolution: Resolution = resolutions.iter().fold(
        Resolution {
            width: 0,
            height: 0,
        },
        |acc, r| Resolution {
            height: if acc.height > r.height {
                acc.height
            } else {
                r.height
            },
            width: if acc.width > r.width {
                acc.width
            } else {
                r.width
            },
        },
    );

    if config.verbose {
        println!("Combined maximum resolution {:?}", combined_resolution);
    }

    resolutions.insert(0, combined_resolution);
    resolutions = resolutions.into_iter().unique().collect_vec();

    return resolutions;
}

fn get_valid_screens_for_recording(config: &Config) -> Vec<SwayWorkspace> {
    let command = "swaymsg -t get_workspaces";
    let output = Command::new("sh")
        .args(&["-c", command])
        .output()
        .expect("Error running swaymsg");

    let stdout_string =
        String::from_utf8(output.stdout).expect("Invalid UTF-8 from get_workspaces");
    let mut workspaces: Vec<SwayWorkspace> =
        serde_json::from_str(stdout_string.as_str()).expect("Invalid json from get_workspaces");

    if config.verbose {
        println!("Found workspaces");
        for elem in workspaces.iter() {
            println!("{:?}", elem);
        }
    }

    workspaces = workspaces
        .into_iter()
        .filter(|w| {
            w.visible
                && !config
                    .screen_blacklist
                    .iter()
                    .any(|screen| screen.eq(&w.output))
                && !config.workspace_blacklist.iter().any(|&num| num == w.num)
        })
        .collect();

    if config.verbose {
        println!("Filtered workspaces");
        for elem in workspaces.iter() {
            println!("{:?}", elem);
        }
    }

    workspaces.sort_by(|a, b| {
        if a.focused && !b.focused {
            Ordering::Less
        } else if a.focused == b.focused {
            Ordering::Equal
        } else {
            Ordering::Greater
        }
    });

    return workspaces;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config {
        current_output: "".to_string(),
        devices_from: 0,
        last_device_index: 0,
        screen_blacklist: Vec::new(),
        workspace_blacklist: Vec::new(),
        verbose: false,
        resolutions: Vec::new(),
        outputs: HashMap::new(),
    };
    let args: Vec<String> = env::args().collect();

    let mut i = 1;
    loop {
        if i >= args.len() {
            break;
        }

        let arg = &args[i];
        if arg == "--not-ws" {
            i += 1;
            config
                .workspace_blacklist
                .push(args[i].clone().parse::<usize>().unwrap());
        } else if arg == "--not-screen" {
            i += 1;
            config.screen_blacklist.push(args[i].clone());
        } else if arg == "-d" || arg == "--devices-from" {
            i += 1;
            config.devices_from = args[i].clone().parse::<usize>().unwrap();
        } else if arg == "--verbose" {
            config.verbose = true;
        } else if arg == "-v" || arg == "--version" {
            println!("v{}", VERSION);
            std::process::exit(0);
        } else if arg == "-h" || arg == "--help" {
            help();
        } else {
            println!("Unknown option: {}", arg);
            help();
        }
        i += 1;
    }

    config.resolutions = get_resolutions(&mut config);
    config
        .outputs
        .insert(config.resolutions[0], config.devices_from);
    config.last_device_index = config.devices_from;
    let valid_screens = get_valid_screens_for_recording(&config);
    let mut recorders: Vec<Box<Child>> = if valid_screens.len() == 0 {
        stream_black(&mut config)?
    } else {
        let output = get_output(&mut config, valid_screens[0].output.as_str());
        record_screen(&mut config, output)?
    };

    let stdout = match Command::new("sh")
        .args(&["-c", "swaymsg -t subscribe -m \"['window']\""])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()?
        .stdout
    {
        Some(stdout) => stdout,
        None => panic!("Could not open swaymsg stdout"),
    };

    let reader = BufReader::new(stdout);
    reader.lines().filter_map(|line| line.ok()).for_each(|_| {
        println!("Switched focus");
        let valid_screens = get_valid_screens_for_recording(&config);
        if valid_screens.len() > 0 && valid_screens[0].output == config.current_output {
            return;
        }
        for recorder in recorders.iter_mut() {
            if config.verbose {
                println!("Killing child");
            }
            match recorder.kill() {
                Ok(_) => {}
                Err(err) => panic!("{:?}", err),
            };
        }

        recorders = if valid_screens.len() == 0 {
            stream_black(&mut config).unwrap()
        } else {
            let output = get_output(&mut config, valid_screens[0].output.as_str());
            record_screen(&mut config, output).unwrap()
        };

        println!("Recording {}", config.current_output);
    });

    Ok(())
}
