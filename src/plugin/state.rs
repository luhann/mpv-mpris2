use crate::mpv::{self, Mpv};
use derive_deftly::{Deftly, derive_deftly_adhoc};
use enumflags2::{BitFlags, bitflags};
use smol::lock::RwLock;
use std::{collections::BTreeMap, path::PathBuf};
use strum::IntoEnumIterator;
use url::Url;

#[derive(Clone, Debug, Default, Deftly)]
#[derive_deftly_adhoc]
pub(crate) struct State {
    pub(crate) fullscreen: bool,
    pub(crate) playlist_entry_id: Option<i64>,
    pub(crate) playlist_current_pos: Option<u64>,
    pub(crate) playlist_count: Option<u64>,
    pub(crate) seekable: bool,
    pub(crate) idle_active: bool,
    pub(crate) eof_reached: bool,
    pub(crate) pause: bool,
    pub(crate) loop_file: bool,
    pub(crate) loop_playlist: bool,
    pub(crate) speed: f64,
    pub(crate) shuffle: bool,
    pub(crate) volume: f64,
    pub(crate) duration: mpv::Seconds,
    pub(crate) media_title: String,
    pub(crate) metadata: BTreeMap<mpv::MetadataKey, String>,
    pub(crate) path: Option<mpv::Path>,
    pub(crate) working_directory: Option<PathBuf>,
    pub(crate) art_url: Option<Url>,
    /// Where `art_url` comes from. Cleared by `start-file`, so each track's
    /// art is worked out again.
    #[deftly(skip)]
    pub(super) art: Option<super::art::Track>,
    #[deftly(skip)]
    pub(crate) track_list: Vec<mpv::Track>,
}

impl super::Player {
    pub(crate) async fn new(mpv: Mpv) -> Result<Self, mpv::Error> {
        // Observe every property `State::change` handles. mpv sends each one's
        // current value as a property-change event, which `main_loop` applies.
        for name in mpv::KnownPropertyName::iter() {
            mpv.observe_property(name).await?;
        }
        let state = RwLock::new(State::default());
        Ok(Self { mpv, state })
    }

    pub(crate) async fn state(&self) -> State {
        self.state.read().await.clone()
    }
}

impl State {
    pub(crate) fn change(&mut self, property: mpv::KnownProperty) {
        use mpv::KnownProperty;
        match property {
            KnownProperty::Fullscreen(fullscreen) => {
                self.fullscreen = fullscreen.unwrap_or_default();
            }
            KnownProperty::PlaylistCurrentPos(playlist_current_pos) => {
                // Negative means there is no current entry.
                self.playlist_current_pos =
                    playlist_current_pos.and_then(|pos| u64::try_from(pos).ok());
            }
            KnownProperty::PlaylistCount(playlist_count) => {
                self.playlist_count = playlist_count;
            }
            KnownProperty::Seekable(seekable) => {
                self.seekable = seekable.unwrap_or_default();
            }
            KnownProperty::IdleActive(idle_active) => {
                self.idle_active = idle_active.unwrap_or_default();
            }
            KnownProperty::EofReached(eof_reached) => {
                self.eof_reached = eof_reached.unwrap_or_default();
            }
            KnownProperty::Pause(pause) => {
                self.pause = pause.unwrap_or_default();
            }
            KnownProperty::LoopFile(loop_file) => {
                self.loop_file = loop_file.map(bool::from).unwrap_or_default();
            }
            KnownProperty::LoopPlaylist(loop_playlist) => {
                self.loop_playlist = loop_playlist.map(bool::from).unwrap_or_default();
            }
            KnownProperty::Speed(speed) => {
                self.speed = speed.unwrap_or_default();
            }
            KnownProperty::Shuffle(shuffle) => {
                self.shuffle = shuffle.unwrap_or_default();
            }
            KnownProperty::Volume(volume) => {
                self.volume = volume.unwrap_or_default();
            }
            KnownProperty::Duration(duration) => {
                self.duration = duration.unwrap_or_default();
            }
            KnownProperty::MediaTitle(media_title) => {
                self.media_title = media_title.unwrap_or_default();
            }
            KnownProperty::Metadata(metadata) => {
                self.metadata = metadata.unwrap_or_default();
            }
            KnownProperty::TrackList(track_list) => {
                self.track_list = track_list.unwrap_or_default();
            }
            KnownProperty::Path(path) => {
                self.path = path;
            }
            KnownProperty::WorkingDirectory(working_directory) => {
                self.working_directory = working_directory;
            }
        }
    }
}

derive_deftly_adhoc! {
    State:

    #[bitflags]
    #[repr(u32)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) enum StateDiff {
        $(
            ${when not(fmeta(skip))}
            ${pascal_case $fname},
        )
    }

    impl State {
        pub(super) fn diff(&self, other: &Self) -> BitFlags<StateDiff> {
            let mut diff = BitFlags::empty();
            $(
                ${when not(fmeta(skip))}
                if self.$fname != other.$fname {
                    diff |= StateDiff::${pascal_case $fname};
                }
            )
            diff
        }
    }
}
