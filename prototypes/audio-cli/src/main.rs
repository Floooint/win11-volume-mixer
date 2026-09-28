//! 阶段 1 音频原型：在命令行中验证 Core Audio 读写与变化监听。

mod audio;
mod metrics;

use std::io::{self, BufRead};

use audio::Command;

const HELP: &str = "\
命令：
  list / ls              显示设备、系统音量和应用列表
  refresh                重新枚举会话后显示列表
  master <0-100>         设置系统音量
  master mute|unmute     系统静音 / 取消静音
  app <序号> <0-100>     设置应用音量（序号来自 list）
  app <序号> mute|unmute 应用静音 / 取消静音
  mem                    显示本进程内存与 CPU 时间
  help                   显示本帮助
  quit / q               退出

在系统音量合成器中调节音量、播放或关闭音频、切换输出设备，这里会实时打印变化。";

enum Input {
    Audio(Command),
    Metrics,
    Help,
    Quit,
}

fn main() {
    println!("Win11 声音控制器 · 音频原型（输入 help 查看命令）");
    let audio = audio::spawn();
    audio.send(Command::List);

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        match parse(&line) {
            Ok(Some(Input::Audio(command))) => audio.send(command),
            Ok(Some(Input::Metrics)) => println!("{}", metrics::report()),
            Ok(Some(Input::Help)) => println!("{HELP}"),
            Ok(Some(Input::Quit)) => break,
            Ok(None) => {}
            Err(message) => println!("{message}"),
        }
    }

    audio.shutdown();
}

fn parse(line: &str) -> Result<Option<Input>, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let input = match words.as_slice() {
        [] => return Ok(None),
        ["list" | "ls"] => Input::Audio(Command::List),
        ["refresh"] => Input::Audio(Command::Refresh),
        ["master", "mute"] => Input::Audio(Command::SetMasterMute(true)),
        ["master", "unmute"] => Input::Audio(Command::SetMasterMute(false)),
        ["master", value] => Input::Audio(Command::SetMasterVolume(parse_volume(value)?)),
        ["app", index, "mute"] => Input::Audio(Command::SetAppMute(parse_index(index)?, true)),
        ["app", index, "unmute"] => Input::Audio(Command::SetAppMute(parse_index(index)?, false)),
        ["app", index, value] => Input::Audio(Command::SetAppVolume(
            parse_index(index)?,
            parse_volume(value)?,
        )),
        ["mem"] => Input::Metrics,
        ["help" | "?"] => Input::Help,
        ["quit" | "exit" | "q"] => Input::Quit,
        _ => return Err("无法识别的命令，输入 help 查看用法".into()),
    };
    Ok(Some(input))
}

fn parse_volume(text: &str) -> Result<f32, String> {
    match text.trim_end_matches('%').parse::<f32>() {
        Ok(value) if (0.0..=100.0).contains(&value) => Ok(value / 100.0),
        _ => Err("音量必须是 0 到 100 之间的数字".into()),
    }
}

fn parse_index(text: &str) -> Result<usize, String> {
    text.parse().map_err(|_| "序号必须是非负整数".to_string())
}
