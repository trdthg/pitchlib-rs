extern crate hound;
extern crate rustfft;

use colored::Colorize;
use cpal::StreamConfig;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::collections::HashMap;
use std::f32::consts::PI;

use std::sync::mpsc::channel;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;

fn fft() {
    // 获取默认音频输入设备
    let host = cpal::default_host();
    let default_input_device = host
        .default_input_device()
        .expect("Failed to get default input device");

    // 获取默认输入设备的配置
    let default_input_format = default_input_device
        .default_input_config()
        .expect("Failed to get default input format");

    // 打印输入设备信息
    println!(
        "Default input device: {}",
        default_input_device.name().unwrap()
    );
    println!("Default input format: {:?}", default_input_format);

    // 打开 WAV 文件写入器
    let spec = hound::WavSpec {
        channels: default_input_format.channels(),
        sample_rate: default_input_format.sample_rate().0,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let writer =
        hound::WavWriter::create("output.wav", spec.clone()).expect("Failed to create WAV writer");
    let writer = Arc::new(Mutex::new(Some(writer)));

    // 采样率 44100
    let sample_rate = default_input_format.sample_rate().0 as usize;

    let clone = writer.clone();
    let (tx, rx) = channel::<Vec<f32>>();

    let mut buffer: Vec<f32> = Vec::new();

    thread::spawn(move || {
        // FFT 大小
        let fft_size = 4096;
        // 创建 FFT 规划器
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);
        let input_buffer_size = fft_size * 8 as usize;

        while let Ok(data) = rx.recv() {
            buffer.extend(data);
            if buffer.len() < input_buffer_size {
                continue;
            }

            // 将音频数据转换为复数形式
            // let mut input = Vec::new();
            let mut input: Vec<Complex<f32>> = Vec::with_capacity(fft_size * 2);
            // 填充输入数据（示例中使用随机数据填充）
            for &x in buffer[..input_buffer_size].iter() {
                input.push(Complex::new(x, 0.0));
            }

            apply_hanning_window(&mut input, fft_size);

            // 执行 FFT 变换
            fft.process(&mut input);

            let mut m = HashMap::new();
            for (hz, db) in
                calculate_frequency_and_db(&input[..input.len() / 2], sample_rate, fft_size)
            {
                if hz > 0. && hz < 500. && db > 10. {
                    m.insert(hz as usize, db as usize);
                }
            }

            let mut map_vec: Vec<(usize, usize)> = {
                let mut res = Vec::new();
                for (key, value) in m.iter() {
                    res.push((*key, *value));
                }
                res
            };

            map_vec.sort_by_key(|&(_, value)| (value as i32) * -1);
            map_vec.truncate(10);
            if map_vec.len() > 0 {
                // map_vec.sort_by_key(|&(key, _)| (key as i32) * 1);

                let mut max_db_index = 0;
                let mut max_db = 0;
                let mut new_line = false;
                for (i, &(_hz, db)) in map_vec.iter().enumerate() {
                    if db > max_db {
                        max_db = db;
                        max_db_index = i;
                    }
                    new_line = true;
                }

                print!(
                    "[{}] ",
                    format!("{}", map_vec[max_db_index].0 as usize)
                        .as_str()
                        .red()
                        .on_blue()
                );

                for (i, &(hz, db)) in map_vec.iter().enumerate() {
                    if i == max_db_index {
                        print!(
                            "| {}:{} ",
                            format!("{}", hz as usize).as_str().red(),
                            db as usize
                        );
                    } else {
                        print!("| {}:{} ", hz as usize, db as usize);
                    }

                    new_line = true;
                    break;
                }
                if new_line {
                    println!();
                }
            }

            buffer = buffer[input_buffer_size..].to_vec()
        }
    });

    // 创建音频输入流
    let input_stream = default_input_device
        .build_input_stream(
            &StreamConfig {
                channels: 1,
                ..default_input_format.config()
            },
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                clone.lock().unwrap().as_mut().and_then(|w| {
                    // 将缓冲区写入 WAV 文件
                    for d in data {
                        w.write_sample(*d).unwrap();
                    }
                    // 确保所有数据都被写入文件
                    w.flush().unwrap();

                    tx.send(data.to_vec()).unwrap();

                    Some(w)
                });
            },
            err_fn,
            None,
        )
        .expect("Failed to build input stream");

    // 启动音频输入流
    input_stream.play().expect("Failed to play input stream");

    // 等待音频输入流结束
    thread::sleep(Duration::from_secs(100));

    if let Ok(mut l) = writer.lock() {
        if let Some(w) = l.take() {
            if let Err(e) = w.finalize() {
                println!("err: {}", e);
            }
        };
    };
    dbg!(spec);
}

// 应用汉宁窗口函数
fn apply_hanning_window(input: &mut [Complex<f32>], window_size: usize) {
    let window = |n: usize, window_size: usize| {
        0.5 * (1.0 - (2.0 * PI as f32 * n as f32 / (window_size - 1) as f32).cos())
    };

    for i in 0..input.len() {
        let value = window(i % window_size, window_size) * input[i].re;
        input[i] = Complex::new(value, 0.0);
    }
}

fn calculate_frequency_and_db(
    spectrum: &[Complex<f32>],
    sample_rate: usize,
    window_size: usize,
) -> Vec<(f32, f32)> {
    let mut res = Vec::with_capacity(window_size);

    for i in 0..window_size {
        // 计算频率
        let freq = i as f32 * sample_rate as f32 / window_size as f32;

        // 计算分贝
        let power = spectrum[i].norm_sqr(); // 计算功率
        let db = 20.0 * power.log10(); // 转换为分贝
        res.push((freq, db));
    }

    res
}

fn main() {
    fft();
}

// 错误处理函数
fn err_fn(err: cpal::StreamError) {
    eprintln!("Error: {}", err);
}

#[cfg(test)]
mod test {
    use std::{f32::consts::PI, thread};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    #[test]
    fn sound() {
        thread::spawn(move || {
            let host = cpal::default_host();
            let device = host
                .default_output_device()
                .expect("no output device available");
            let config = device.default_output_config().unwrap();

            let sample_rate = config.sample_rate().0 as f32;
            let duration_secs = 5; // 持续时间（秒）
            let frequency = 200.0; // 频率（Hz）

            let num_samples = (duration_secs as f32 * sample_rate) as usize;

            let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);

            match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let stream = device.build_output_stream(
                        &config.into(),
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            for (i, sample) in data.iter_mut().enumerate() {
                                let t_sec = i as f32 / sample_rate;
                                *sample = (2.0 * PI * frequency * t_sec).sin();
                            }
                        },
                        err_fn,
                        None,
                    );
                    stream.expect("failed to build stream").play().unwrap();
                }
                cpal::SampleFormat::I16 => {
                    let stream = device.build_output_stream(
                        &config.into(),
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            for (i, sample) in data.iter_mut().enumerate() {
                                let t_sec = i as f32 / sample_rate;
                                let amplitude = i16::MAX as f32;
                                *sample = (amplitude * (2.0 * PI * frequency * t_sec).sin()) as i16;
                            }
                        },
                        err_fn,
                        None,
                    );
                    stream.expect("failed to build stream").play().unwrap();
                }
                _ => panic!("unsupported sample format"),
            };
        });
    }
}
