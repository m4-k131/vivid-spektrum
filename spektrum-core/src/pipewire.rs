use crate::error::CoreError;
use crate::ring::{SampleRing, SampleRingProducer};
use pipewire as pw;
use std::sync::Mutex;
use pw::properties::properties;
use pw::spa;
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::pod::Pod;
use std::convert::TryInto;
use std::mem;

struct UserData {
    format: spa::param::audio::AudioInfoRaw,
    ring: SampleRing,
}

pub fn spawn_capture(target: Option<String>, ring: SampleRing) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if let Err(e) = run_capture(target, ring) {
            eprintln!("pipewire capture ended: {}", e);
        }
    })
}

struct LockfreeUserData {
    format: spa::param::audio::AudioInfoRaw,
    producer: Mutex<SampleRingProducer>,
}

pub fn spawn_capture_lockfree(target: Option<String>, producer: SampleRingProducer) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if let Err(e) = run_capture_lockfree(target, producer) {
            eprintln!("pipewire capture ended: {}", e);
        }
    })
}

fn run_capture_lockfree(target: Option<String>, producer: SampleRingProducer) -> Result<(), CoreError> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let data = LockfreeUserData {
        format: Default::default(),
        producer: Mutex::new(producer),
    };
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
    };
    props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
    if let Some(t) = target {
        props.insert("target.object", t);
    }
    let stream = pw::stream::StreamBox::new(&core, "vividspektrum-capture", props)?;
    let _listener = stream
        .add_local_listener_with_user_data(data)
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            let (media_type, media_subtype) = match format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            let _ = user_data.format.parse(param);
        })
        .process(|stream, user_data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let bpe = mem::size_of::<f32>();
            let n_ch = user_data.format.channels().max(1) as usize;
            let frame_bytes = data.chunk().size() as usize;
            let n_frames = frame_bytes / (bpe * n_ch);
            if let Some(samples) = data.data() {
                let need = n_frames * n_ch * bpe;
                if samples.len() < need {
                    return;
                }
                let mut interleaved = Vec::with_capacity(n_frames * n_ch);
                for chunk in samples[..need].chunks_exact(bpe) {
                    interleaved.push(f32::from_le_bytes(chunk.try_into().unwrap()));
                }
                let _ = user_data.producer.lock().unwrap().push_interleaved(&interleaved, n_ch);
            }
        })
        .register()?;
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .unwrap()
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&values).unwrap()];
    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;
    mainloop.run();
    Ok(())
}

fn run_capture(target: Option<String>, ring: SampleRing) -> Result<(), CoreError> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let data = UserData {
        format: Default::default(),
        ring,
    };
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
    };
    props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
    if let Some(t) = target {
        props.insert("target.object", t);
    }
    let stream = pw::stream::StreamBox::new(&core, "vividspektrum-capture", props)?;
    let _listener = stream
        .add_local_listener_with_user_data(data)
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            let (media_type, media_subtype) = match format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            let _ = user_data.format.parse(param);
        })
        .process(|stream, user_data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let bpe = mem::size_of::<f32>();
            let n_ch = user_data.format.channels().max(1) as usize;
            let frame_bytes = data.chunk().size() as usize;
            let n_frames = frame_bytes / (bpe * n_ch);
            if let Some(samples) = data.data() {
                let need = n_frames * n_ch * bpe;
                if samples.len() < need {
                    return;
                }
                let mut interleaved = Vec::with_capacity(n_frames * n_ch);
                for chunk in samples[..need].chunks_exact(bpe) {
                    interleaved.push(f32::from_le_bytes(chunk.try_into().unwrap()));
                }
                let _ = user_data.ring.push_interleaved(&interleaved, n_ch);
            }
        })
        .register()?;
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .unwrap()
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&values).unwrap()];
    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;
    mainloop.run();
    Ok(())
}
