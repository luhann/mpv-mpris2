use crate::mpv::{self, Mpv};
use futures_concurrency::stream::Merge;
use mpris_server::Signal;
use smol::{LocalExecutor, Timer, lock::RwLock, prelude::*};
use std::time::{Duration, Instant};
use url::Url;

pub(crate) mod args;
mod art;
mod mpris;
mod state;

pub(crate) struct Player {
    state: RwLock<state::State>,
    mpv: Mpv,
}

/// After each event, wait this long for more before sending the changes. mpv
/// reports a track change in several bursts a few milliseconds apart, and
/// sending each one makes clients briefly show half-updated metadata.
const QUIET_PERIOD: Duration = Duration::from_millis(50);

/// Send changes at most this long after the first event, even if more events
/// keep arriving.
const MAX_DELAY: Duration = Duration::from_millis(250);

pub(crate) async fn main_loop(
    ex: &LocalExecutor<'_>,
    server: mpris_server::Server<Player>,
    events: kanal::AsyncReceiver<Vec<mpv::Event>>,
) -> anyhow::Result<()> {
    enum LoopEvent {
        MpvEvents(Vec<mpv::Event>),
        ArtFile(art::Extracted),
    }
    let (mut art, art_files) = art::State::new();
    let mut loop_events = {
        let events = events.stream().map(LoopEvent::MpvEvents);
        let art_files = art_files.stream().map(LoopEvent::ArtFile);
        (events, art_files).merge()
    };
    while let Some(first) = loop_events.next().await {
        let mut state = server.imp().state().await;
        let mut seeked = None;
        let send_by = Instant::now() + MAX_DELAY;
        let mut next = Some(first);
        // Apply events until there's a quiet period, then send one update.
        while let Some(loop_event) = next {
            match loop_event {
                LoopEvent::MpvEvents(events) => {
                    use mpv::{Event, Property};
                    for event in events {
                        match event {
                            Event::Shutdown => return Ok(()),
                            Event::StartFile {
                                playlist_entry_id: value,
                            } => {
                                state.art = None;
                                state.art_url = None;
                                state.playlist_entry_id = Some(value);
                                art.clear();
                            }
                            Event::EndFile => {
                                state.playlist_entry_id = None;
                            }
                            Event::PropertyChange(Property::Known(property)) => {
                                state.change(property);
                            }
                            // Every observed property is a `KnownProperty`, so this
                            // means its value didn't match the expected type.
                            Event::PropertyChange(Property::Unknown { name, data }) => {
                                tracing::error!(%name, %data, "Failed to parse property change");
                            }
                            Event::Seeked { playback_time } => {
                                seeked = Some(playback_time);
                            }
                            _ => (),
                        }
                    }
                }
                // Ignore art extracted for an earlier track.
                LoopEvent::ArtFile(extracted) if state.art.as_ref() == Some(&extracted.source) => {
                    state.art_url = Url::from_file_path(extracted.file.path()).ok();
                    art.set_file(extracted.file);
                }
                LoopEvent::ArtFile(_) => (),
            }
            let send_at = (Instant::now() + QUIET_PERIOD).min(send_by);
            next = loop_events
                .next()
                .or(async {
                    Timer::at(send_at).await;
                    None
                })
                .await;
        }
        if let Some(playback_time) = seeked.take()
            && let Err(e) = server
                .emit(Signal::Seeked {
                    position: playback_time.into(),
                })
                .await
        {
            tracing::error!(error = %e, "Failed to emit seeked signal");
        }
        let changes = server.imp().update(state).await;
        if let Some((path, index)) = changes.embedded_art {
            art.spawn_worker(ex, path, index);
        }
        if let Err(e) = server.properties_changed(changes.properties).await {
            tracing::error!(error = %e, "Failed to emit changes");
        }
    }
    Ok(())
}
