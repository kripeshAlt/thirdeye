use anyhow::{Context, Result};
use clap::Parser;
use eframe::egui;
use image::{DynamicImage, ImageBuffer, RgbImage};
use rqrr::PreparedImage;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// QR Code Scanner - A cross-platform desktop app for scanning QR codes from webcam
#[derive(Parser, Debug)]
#[command(name = "qr-scanner", version, about = "Scan QR codes from webcam")]
struct Args {
    /// Camera device index (default: 0)
    #[arg(short, long, default_value_t = 0)]
    device: usize,

    /// Camera width resolution (default: 1280)
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Camera height resolution (default: 720)
    #[arg(long, default_value_t = 720)]
    height: u32,

    /// Output file for last scanned QR code (default: qr_result.txt)
    #[arg(short, long, default_value = "qr_result.txt")]
    output: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    log::info!("Starting QR Code Scanner");
    log::debug!("Args: {:?}", args);

    // Initialize camera
    let camera = Camera::new(args.device, args.width, args.height)
        .context("Failed to initialize camera")?;

    let app = QrScannerApp::new(camera, args.output);
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("QR Code Scanner"),
        ..Default::default()
    };

    eframe::run_native(
        "QR Code Scanner",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .context("Failed to run eframe application")?;

    Ok(())
}

/// Camera abstraction for cross-platform webcam access
struct Camera {
    #[cfg(target_os = "linux")]
    device: v4l::Device,
    #[cfg(target_os = "windows")]
    device: Option<windows::Media::Capture::MediaCapture>,
    #[cfg(target_os = "macos")]
    device: Option<std::sync::Arc<dyn CameraTrait>>,
    width: u32,
    height: u32,
}

impl Camera {
    fn new(device_index: usize, width: u32, height: u32) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let device = v4l::Device::new(device_index)
                .context("Failed to open video device")?;
            device.set_format(&v4l::Format::new(width, height, v4l::FourCC::new(b"RGB3")))
                .context("Failed to set video format")?;
            Ok(Self { device, width, height })
        }

        #[cfg(target_os = "windows")]
        {
            // Windows MediaCapture initialization would go here
            // For now, return a placeholder
            Ok(Self { device: None, width, height })
        }

        #[cfg(target_os = "macos")]
        {
            // macOS AVCaptureSession initialization would go here
            Ok(Self { device: None, width, height })
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            anyhow::bail!("Unsupported platform")
        }
    }

    /// Capture a single frame from the camera
    fn capture_frame(&mut self) -> Result<DynamicImage> {
        #[cfg(target_os = "linux")]
        {
            let mut buffer = vec![0u8; (self.width * self.height * 3) as usize];
            self.device.read(&mut buffer).context("Failed to read frame")?;
            let img = ImageBuffer::<image::Rgb<u8>, _>::from_raw(self.width, self.height, buffer)
                .context("Failed to create image from buffer")?;
            Ok(DynamicImage::ImageRgb8(img))
        }

        #[cfg(target_os = "windows")]
        {
            // Windows frame capture implementation
            // Return a placeholder for now
            Ok(DynamicImage::new_rgb8(self.width, self.height))
        }

        #[cfg(target_os = "macos")]
        {
            // macOS frame capture implementation
            Ok(DynamicImage::new_rgb8(self.width, self.height))
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            anyhow::bail!("Unsupported platform")
        }
    }
}

/// Trait for platform-specific camera implementations
#[cfg(target_os = "macos")]
trait CameraTrait: Send + Sync {
    fn capture(&self) -> Result<DynamicImage>;
}

/// Main application state
struct QrScannerApp {
    camera: Arc<Mutex<Camera>>,
    last_frame: Arc<Mutex<Option<DynamicImage>>>,
    last_qr_result: Arc<Mutex<Option<String>>>,
    output_path: PathBuf,
    scanning: bool,
    last_scan_time: Instant,
    scan_cooldown: Duration,
    error_message: Arc<Mutex<Option<String>>>,
}

impl QrScannerApp {
    fn new(camera: Camera, output_path: PathBuf) -> Self {
        let camera = Arc::new(Mutex::new(camera));
        let camera_clone = camera.clone();
        let last_frame = Arc::new(Mutex::new(None));
        let last_frame_clone = last_frame.clone();
        let last_qr_result = Arc::new(Mutex::new(None));
        let last_qr_result_clone = last_qr_result.clone();
        let error_message = Arc::new(Mutex::new(None));
        let error_message_clone = error_message.clone();

        // Spawn background thread for continuous frame capture and QR scanning
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(33)); // ~30 FPS

                // Capture frame
                let frame = {
                    let mut cam = match camera_clone.lock() {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    match cam.capture_frame() {
                        Ok(f) => f,
                        Err(e) => {
                            *error_message_clone.lock().unwrap() = Some(format!("Capture error: {}", e));
                            continue;
                        }
                    }
                };

                // Update last frame for display
                *last_frame_clone.lock().unwrap() = Some(frame.clone());

                // Scan for QR codes (with cooldown to avoid excessive CPU usage)
                // We'll scan every ~500ms instead of every frame
            }
        });

        Self {
            camera,
            last_frame,
            last_qr_result,
            output_path,
            scanning: true,
            last_scan_time: Instant::now(),
            scan_cooldown: Duration::from_millis(500),
            error_message,
        }
    }

    fn scan_qr_code(&self, image: &DynamicImage) -> Option<String> {
        let gray = image.to_luma8();
        let prepared = PreparedImage::prepare(gray);
        let grids = prepared.detect_grids();

        for grid in grids {
            if let Ok((meta, content)) = grid.decode() {
                log::info!("QR Code detected: version={}, ecc={:?}", meta.version, meta.ecc_level);
                return Some(content);
            }
        }
        None
    }
}

impl eframe::App for QrScannerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Scan for QR codes periodically
        if self.scanning && self.last_scan_time.elapsed() >= self.scan_cooldown {
            if let Some(frame) = self.last_frame.lock().unwrap().as_ref() {
                if let Some(qr_content) = self.scan_qr_code(frame) {
                    *self.last_qr_result.lock().unwrap() = Some(qr_content.clone());

                    // Save to file
                    if let Err(e) = std::fs::write(&self.output_path, &qr_content) {
                        log::error!("Failed to write QR result: {}", e);
                    } else {
                        log::info!("QR result saved to {:?}", self.output_path);
                    }
                }
            }
            self.last_scan_time = Instant::now();
        }

        // Request repaint for continuous updates
        ctx.request_repaint();

        // Top panel - controls
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("📷 QR Code Scanner");

                ui.separator();

                let scanning = self.scanning;
                if ui
                    .add(egui::Button::new(if scanning { "⏸ Pause" } else { "▶ Resume" }))
                    .clicked()
                {
                    self.scanning = !scanning;
                }

                if ui.button("💾 Save Last Result").clicked() {
                    if let Some(result) = self.last_qr_result.lock().unwrap().as_ref() {
                        if let Err(e) = std::fs::write(&self.output_path, result) {
                            *self.error_message.lock().unwrap() = Some(format!("Save failed: {}", e));
                        } else {
                            *self.error_message.lock().unwrap() = Some("Result saved!".to_string());
                        }
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(err) = self.error_message.lock().unwrap().as_ref() {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                });
            });
        });

        // Central panel - camera feed
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(frame) = self.last_frame.lock().unwrap().as_ref() {
                // Convert DynamicImage to egui texture
                let size = [frame.width() as usize, frame.height() as usize];
                let pixels: Vec<egui::Color32> = frame
                    .to_rgb8()
                    .pixels()
                    .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
                    .collect();

                let image = egui::ColorImage::from_rgb(size, &pixels);
                let texture = ctx.load_texture("camera_feed", image, egui::TextureOptions::LINEAR);

                let available_size = ui.available_size();
                let aspect_ratio = frame.width() as f32 / frame.height() as f32;
                let display_width = available_size.x.min(available_size.y * aspect_ratio);
                let display_height = display_width / aspect_ratio;

                ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(display_width, display_height)));
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Initializing camera...");
                });
            }
        });

        // Bottom panel - QR code result
        egui::TopBottomPanel::bottom("result").show(ctx, |ui| {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Last QR Code:");
                if let Some(result) = self.last_qr_result.lock().unwrap().as_ref() {
                    ui.monospace(result);
                    if ui.button("📋 Copy").clicked() {
                        ui.output_mut(|o| o.copied_text = result.clone());
                    }
                } else {
                    ui.weak("No QR code detected yet");
                }
            });
        });
    }
}