use std::env;
use std::fs::File;
use std::io::Write;

extern crate ffmpeg_next as ffmpeg;

use anyhow::anyhow;
use anyhow::Context as _;
use anyhow::Result;
use ffmpeg::format::input;
use ffmpeg::format::Pixel;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::context::Context;
use ffmpeg::software::scaling::flag::Flags;
use ffmpeg::util::frame::video::Video;
use ffmpeg::Error;

fn main() -> Result<(), anyhow::Error> {
    ffmpeg::init().unwrap();

    let input_path = env::args_os().nth(1).expect("usage: input-file");
    let mut ictx = input(&input_path).with_context(|| anyhow!("opening {:?}", input_path))?;
    let input = ictx
        .streams()
        .best(Type::Video)
        .ok_or_else(|| ffmpeg::Error::StreamNotFound)?;
    let video_stream_index = input.index();

    let mut decoder = input.codec().decoder().video()?;

    let mut scaler = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        Flags::BILINEAR,
    )?;

    let mut frame_index = 0;

    for (stream, packet) in ictx.packets() {
        if stream.index() != video_stream_index {
            continue;
        }
        match decoder.send_packet(&packet) {
            Ok(()) => (),
            Err(Error::InvalidData) => (),
            Err(e) => Err(e).with_context(|| anyhow!("sending packet"))?,
        };
        if receive_and_process_decoded_frames(&mut frame_index, &mut scaler, &mut decoder)
            .with_context(|| anyhow!("processing frame {}", frame_index))?
        {
            break;
        }
    }
    decoder.send_eof()?;
    receive_and_process_decoded_frames(&mut frame_index, &mut scaler, &mut decoder)?;

    Ok(())
}
/** @return true if we're at the end of file */
fn receive_and_process_decoded_frames(
    frame_index: &mut u64,
    scaler: &mut Context,
    decoder: &mut ffmpeg::decoder::Video,
) -> Result<bool> {
    let mut decoded = Video::empty();
    loop {
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => (),
            // EAGAIN - not sure why this isn't a wrapped value, or why it is getting back out here
            Err(Error::Other { errno }) if errno == 11 => break,
            Err(Error::Eof) => return Ok(true),
            Err(e) => Err(e).with_context(|| anyhow!("receiving frame"))?,
        }
        let mut rgb_frame = Video::empty();
        scaler
            .run(&decoded, &mut rgb_frame)
            .with_context(|| anyhow!("scaling frame"))?;
        if false {
            save_file(&rgb_frame, *frame_index)?;
        }
        *frame_index += 1;
    }
    Ok(false)
}

fn save_file(frame: &Video, index: u64) -> Result<()> {
    let mut file = File::create(format!("frame{}.ppm", index))?;
    file.write_all(format!("P6\n{} {}\n255\n", frame.width(), frame.height()).as_bytes())?;
    file.write_all(frame.data(0))?;
    Ok(())
}
