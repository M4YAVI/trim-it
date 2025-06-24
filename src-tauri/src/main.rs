#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use tauri::{Window, Emitter};
use url::Url;
use chrono;
use tempfile;
use tokio::process::Command;

#[tauri::command]
async fn ensure_ffmpeg_is_ready(window: Window) -> Result<(), String> {
    let mut test_command = ffmpeg_sidecar::command::FfmpegCommand::new();
    
    let spawn_result = test_command
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("nullsrc=d=0.1")
        .arg("-t")
        .arg("0.1")
        .arg("-f")
        .arg("null")
        .arg("-")
        .spawn();

    match spawn_result {
        Ok(mut child) => {
            let success = child.iter()
                .map_err(|e| e.to_string())?
                .any(|event| matches!(event, ffmpeg_sidecar::event::FfmpegEvent::Done));
            
            if success {
                let _ = window.emit("ffmpeg_status", "FFmpeg is ready.");
                Ok(())
            } else {
                let _ = window.emit("ffmpeg_status", "FFmpeg not working properly.");
                Err("FFmpeg did not complete successfully.".to_string())
            }
        }
        Err(e) => {
            let _ = window.emit("ffmpeg_status", "FFmpeg not found. Please install FFmpeg manually.");
            Err(format!("FFmpeg is not installed or failed to spawn: {}. Please ensure it's in your PATH.", e))
        }
    }
}

// Optimized function to download only the required segment from YouTube
async fn download_youtube_video_segment(
    url: &str, 
    output_dir: &Path, 
    start_time: &str, 
    end_time: &str
) -> Result<PathBuf, String> {
    let output_template = output_dir.join("video.mp4");

    // Convert time format from HH:MM:SS to seconds for yt-dlp
    let start_seconds = time_to_seconds(start_time)?;
    let end_seconds = time_to_seconds(end_time)?;
    
    // Create download sections parameter
    let download_sections = format!("*{}-{}", start_seconds, end_seconds);

    let status = Command::new("yt-dlp")
        // Simplified format selection for speed - prefer h264 mp4
        .arg("-f")
        .arg("best[ext=mp4]/best")
        .arg("--download-sections")
        .arg(&download_sections)
        .arg("--force-keyframes-at-cuts")
        // Speed optimizations
        .arg("--concurrent-fragments")
        .arg("4") // Download 4 fragments concurrently
        .arg("--no-mtime") // Don't set file modification time
        .arg("-o")
        .arg(&output_template)
        .arg(url)
        .status()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "yt-dlp command not found. Please install yt-dlp and ensure it is in your system's PATH.".to_string()
            } else {
                format!("Failed to execute yt-dlp: {}", e)
            }
        })?;

    if !status.success() {
        return Err("yt-dlp failed to download the video segment. The URL might be invalid, private, or require a login.".to_string());
    }

    if output_template.exists() {
        Ok(output_template)
    } else {
        Err("yt-dlp ran, but the expected output file was not found.".to_string())
    }
}

// Helper function to convert HH:MM:SS to seconds
fn time_to_seconds(time_str: &str) -> Result<f64, String> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 3 {
        return Err("Invalid time format. Expected HH:MM:SS".to_string());
    }
    
    let hours: f64 = parts[0].parse().map_err(|_| "Invalid hours")?;
    let minutes: f64 = parts[1].parse().map_err(|_| "Invalid minutes")?;
    let seconds: f64 = parts[2].parse().map_err(|_| "Invalid seconds")?;
    
    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

#[tauri::command]
async fn trim_video(
    video_source: String,
    start_time: String,
    end_time: String,
    ratio: String,
) -> Result<String, String> {
    let video_path: PathBuf;
    let _temp_dir_guard: Option<tempfile::TempDir>;
    let is_youtube_video: bool;

    // Check if it's a YouTube video before consuming the string
    is_youtube_video = video_source.contains("youtube.com") || video_source.contains("youtu.be");

    if video_source.starts_with("http") {
        let temp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
        
        // Check for YouTube URLs and download only the segment
        if is_youtube_video {
            video_path = download_youtube_video_segment(
                &video_source, 
                temp_dir.path(), 
                &start_time, 
                &end_time
            ).await?;
        } else {
            // For other direct video links, download the full video
            let parsed_url = Url::parse(&video_source).map_err(|e| format!("Invalid URL: {}", e))?;
            let filename = parsed_url
                .path_segments()
                .and_then(|segments| segments.last())
                .unwrap_or("downloaded_video.mp4")
                .to_string();

            let temp_path = temp_dir.path().join(filename);

            download_video_from_url(&video_source, &temp_path)
                .await
                .map_err(|e| format!("Failed to download video: {}", e))?;

            video_path = temp_path;
        }
        
        _temp_dir_guard = Some(temp_dir);
    } else {
        video_path = PathBuf::from(video_source);
        if !video_path.exists() {
            return Err(format!("Local video file not found: {}", video_path.display()));
        }
        _temp_dir_guard = None;
    }

    let output_dir = if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE")
            .map(|home| PathBuf::from(home).join("Downloads"))
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join("Downloads"))
            .unwrap_or_else(|_| PathBuf::from("."))
    };

    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir).map_err(|e| format!("Failed to create Downloads directory: {}", e))?;
    }

    let output_filename = format!(
        "trimmed_{}.mp4",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );
    let output_path = output_dir.join(output_filename);
    
    let mut command = ffmpeg_sidecar::command::FfmpegCommand::new();
    
    // If it's a YouTube video and we only need to copy (no aspect ratio change)
    if is_youtube_video {
        if ratio == "Original" {
            // Just copy the already-trimmed YouTube video
            command
                .input(&video_path.to_string_lossy())
                .args(&["-c", "copy"])
                .args(&["-movflags", "+faststart"]) // Optimize for web playback
                .output(&output_path.to_string_lossy())
                .overwrite();
        } else {
            // Apply aspect ratio conversion to the YouTube segment
            command.input(&video_path.to_string_lossy());
            apply_aspect_ratio_filter_fast(&mut command, &ratio)?;
            command.output(&output_path.to_string_lossy()).overwrite();
        }
    } else {
        // For non-YouTube videos or local files, do the full trim + conversion
        command
            .input(&video_path.to_string_lossy())
            .arg("-ss")
            .arg(&start_time)
            .arg("-to")
            .arg(&end_time);

        if ratio == "Original" {
            command
                .arg("-c")
                .arg("copy")
                .args(&["-avoid_negative_ts", "make_zero"]) // Fix timestamp issues
                .args(&["-movflags", "+faststart"]); // Optimize for web playback
        } else {
            apply_aspect_ratio_filter_fast(&mut command, &ratio)?;
        }

        command.output(&output_path.to_string_lossy()).overwrite();
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to execute FFmpeg: {}", e))?;

    let mut success = false;
    let mut ffmpeg_errors: Vec<String> = Vec::new();
    for event in child.iter().map_err(|e| e.to_string())? {
        match event {
            ffmpeg_sidecar::event::FfmpegEvent::Done => {
                success = true;
                break;
            }
            ffmpeg_sidecar::event::FfmpegEvent::Error(e) => {
                ffmpeg_errors.push(e.to_string());
            }
            _ => {}
        }
    }

    if success && output_path.exists() {
        Ok(format!("Video trimmed successfully! Saved to: {}", output_path.display()))
    } else {
        if !ffmpeg_errors.is_empty() {
            Err(format!("FFmpeg failed: {}", ffmpeg_errors.join("; ")))
        } else {
            Err("FFmpeg failed to create the output file or did not finish successfully.".to_string())
        }
    }
}

// Optimized helper function for faster video processing
fn apply_aspect_ratio_filter_fast(command: &mut ffmpeg_sidecar::command::FfmpegCommand, ratio: &str) -> Result<(), String> {
    // Use ultrafast preset and higher CRF for speed
    match ratio {
        "16:9" => {
            command.args(&[
                "-vf", "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2",
                "-c:v", "libx264",
                "-preset", "ultrafast", // Fastest encoding preset
                "-crf", "28", // Higher CRF = lower quality but faster
                "-c:a", "aac",
                "-b:a", "128k",
                "-movflags", "+faststart", // Optimize for web playback
            ]);
        }
        "9:16" => {
            command.args(&[
                "-vf", "scale=720:1280:force_original_aspect_ratio=decrease,pad=720:1280:(ow-iw)/2:(oh-ih)/2",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-crf", "28",
                "-c:a", "aac",
                "-b:a", "128k",
                "-movflags", "+faststart",
            ]);
        }
        "1:1" => {
            command.args(&[
                "-vf", "scale=720:720:force_original_aspect_ratio=decrease,pad=720:720:(ow-iw)/2:(oh-ih)/2",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-crf", "28",
                "-c:a", "aac",
                "-b:a", "128k",
                "-movflags", "+faststart",
            ]);
        }
        _ => return Err(format!("Unsupported ratio: {}", ratio)),
    }
    Ok(())
}

async fn download_video_from_url(url: &str, output_path: &PathBuf) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    use futures::StreamExt;

    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to download video: HTTP status {}", response.status()));
    }

    let mut file = tokio::fs::File::create(output_path)
        .await
        .map_err(|e| format!("Failed to create temporary file: {}", e))?;

    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Error while downloading chunk: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write chunk to file: {}", e))?;
    }

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ensure_ffmpeg_is_ready,
            trim_video
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
