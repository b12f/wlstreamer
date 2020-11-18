use std::env;
use std::io::{BufRead, BufReader, Error, ErrorKind, Write};
use std::process::{Child, Command, Stdio};

struct Config {
    current_screen: String,
    output: String,
    screen_blacklist: Vec<String>,
    workspace_blacklist: Vec<String>,
    verbose: bool,
}

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn help() {
    println!("Wrapper around wf-recorder and ffmpeg that automatically switches the screen being recorded based on current window focus");
    println!("");
    println!("Usage: wlstreamer [options]");
    println!("");
    println!("Options:");
    println!("  --not-ws <ws-num>         Do not show this workspace. Can be used multiple times. Example: 3");
    println!("  --not-screen <screen>     Do not show this screen. Can be used multiple times. Example: HDMI-A-1");
    println!("  -o|--output <output>      Output to this device. Defaults to /dev/video0");
    println!("  -v|--version              Display version and exit");
    println!("  --verbose                 Verbose logging");
    println!("");
    println!(
        "If there are no screens available for streaming, a black screen will be shown instead."
    );
    std::process::exit(0);
}

fn record_screen(config: &mut Config, valid_screens: &Vec<String>) -> Result<Child, Error> {
    if valid_screens.len() == 0 {
        let cmd = Command::new("ffmpeg")
            .args(&[
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=1920x1080:r=25/1",
                "-vcodec",
                "rawvideo",
                "-pix_fmt",
                "yuyv422",
                "-f",
                "v4l2",
                config.output.as_str(),
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

        config.current_screen = "".to_string();

        return Ok(cmd);
    } else {
        let index = get_screen_index(&valid_screens[0]);
        let output_str = format!("--file={}", config.output.as_str());
        println!("Outputting to {}", config.output.as_str());
        let mut cmd = Command::new("wf-recorder")
            .args(&[
                "--muxer=v4l2",
                "--codec=rawvideo",
                "--pixel-format=yuyv422",
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

        cmd.stdin
            .as_mut()
            .ok_or(Error::new(ErrorKind::Other, "Recorder process failed"))?
            .write_all(index.as_bytes())?;

        config.current_screen = valid_screens[0].as_str().to_string();

        return Ok(cmd);
    };
}

fn get_valid_screens_for_recording(config: &Config) -> Vec<String> {
    let mut command =
        "swaymsg -t get_workspaces | jq -r 'sort_by(.focused != true) | map(select(.visible"
            .to_string();
    for screen in &config.screen_blacklist {
        command.push_str(" and .output != \"");
        command.push_str(screen.as_str());
        command.push_str("\"");
    }
    for workspace in &config.workspace_blacklist {
        command.push_str(" and .num != ");
        command.push_str(workspace.as_str());
    }
    command.push_str(")) | map(.output) | .[]'");
    let output = Command::new("sh")
        .args(&["-c", command.as_str()])
        .output()
        .expect("Couldn't get current focus");

    let stdout = String::from_utf8(output.stdout).expect("Invalid UTF-8 from get_workspaces");
    let valid_screens = stdout
        .split('\n')
        .map(|s| s.to_string())
        .filter(|s| s != "")
        .collect();

    println!("Found recordable screens: {:?}", valid_screens);

    return valid_screens;
}

fn get_screen_index(screen: &String) -> String {
    let mut command = "swaymsg -t get_outputs | jq -r 'map(.name==\"".to_string();
    command.push_str(screen.as_str());
    command.push_str("\") | index(true) + 1'");
    let output = Command::new("sh")
        .args(&["-c", command.as_str()])
        .output()
        .expect("Couldn't get screen index");

    let stdout = String::from_utf8(output.stdout).expect("Invalid UTF-8 from get_workspaces");
    return stdout;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config {
        current_screen: "".to_string(),
        output: "/dev/video0".to_string(),
        screen_blacklist: Vec::new(),
        workspace_blacklist: Vec::new(),
        verbose: false,
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
            config.workspace_blacklist.push(args[i].clone());
        } else if arg == "--not-screen" {
            i += 1;
            config.screen_blacklist.push(args[i].clone());
        } else if arg == "-o" || arg == "--output" {
            i += 1;
            config.output = args[i].clone();
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

    let valid_screens = get_valid_screens_for_recording(&config);
    let mut recorder = record_screen(&mut config, &valid_screens)?;

    let stdout = match Command::new("sh")
        .args(&["-c", "swaymsg -t subscribe -m \"['window']\""])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()?
        .stdout
    {
        Some(stdout) => stdout,
        None => panic!("Could not open stdout"),
    };

    let reader = BufReader::new(stdout);
    reader.lines().filter_map(|line| line.ok()).for_each(|_| {
        println!("Switched focus");
        let valid_screens = get_valid_screens_for_recording(&config);
        if valid_screens.len() > 0 && valid_screens[0] == config.current_screen {
            return;
        }

        match recorder.kill() {
            Ok(_) => {}
            Err(err) => panic!("{:?}", err),
        };

        recorder = match record_screen(&mut config, &valid_screens) {
            Ok(recorder) => recorder,
            Err(err) => panic!("{:?}", err),
        };
        println!("Recording {}", config.current_screen);
    });

    Ok(())
}
