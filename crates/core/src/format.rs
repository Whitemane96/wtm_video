use std::fmt;
use std::str::FromStr;

/// The file type to save, chosen by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Mp4,
    Mkv,
    Webm,
    Mov,
    Mp3,
    M4a,
    Opus,
    Flac,
    Wav,
}

impl OutputFormat {
    pub const ALL: [OutputFormat; 9] = [
        Self::Mp4,
        Self::Mkv,
        Self::Webm,
        Self::Mov,
        Self::Mp3,
        Self::M4a,
        Self::Opus,
        Self::Flac,
        Self::Wav,
    ];

    /// File extension, which is also the name used on the command line.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
            Self::Mov => "mov",
            Self::Mp3 => "mp3",
            Self::M4a => "m4a",
            Self::Opus => "opus",
            Self::Flac => "flac",
            Self::Wav => "wav",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4 video",
            Self::Mkv => "MKV video",
            Self::Webm => "WebM video",
            Self::Mov => "MOV video",
            Self::Mp3 => "MP3 audio",
            Self::M4a => "M4A audio",
            Self::Opus => "Opus audio",
            Self::Flac => "FLAC audio",
            Self::Wav => "WAV audio",
        }
    }

    pub fn is_audio(self) -> bool {
        matches!(self, Self::Mp3 | Self::M4a | Self::Opus | Self::Flac | Self::Wav)
    }

    /// A caveat worth showing next to the choice.
    pub fn note(self) -> Option<&'static str> {
        match self {
            Self::Mp4 => Some("Best compatibility at 1080p and below. 1440p/4K use VP9 or AV1, which some players can't open."),
            Self::Mov => Some("MOV uses H.264 for compatibility with QuickTime, which is usually limited to 1080p."),
            Self::Webm => Some("WebM keeps YouTube's original VP9/Opus streams."),
            Self::Mkv => Some("MKV keeps the best streams available, but some players can't open it."),
            Self::Wav | Self::Flac => Some("Lossless formats make much larger files."),
            _ => None,
        }
    }

    /// Everything except plain MP4 needs ffmpeg to merge or convert.
    pub fn needs_ffmpeg(self) -> bool {
        self != Self::Mp4
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let wanted = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|f| f.extension() == wanted)
            .ok_or_else(|| {
                let names: Vec<_> = Self::ALL.iter().map(|f| f.extension()).collect();
                format!("unknown format '{s}' (choose from: {})", names.join(", "))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_round_trips_through_its_name() {
        for f in OutputFormat::ALL {
            assert_eq!(f.extension().parse::<OutputFormat>(), Ok(f));
        }
    }

    #[test]
    fn parsing_ignores_case_and_rejects_unknown() {
        assert_eq!("MKV".parse::<OutputFormat>(), Ok(OutputFormat::Mkv));
        assert!("avi".parse::<OutputFormat>().unwrap_err().contains("choose from"));
    }

    #[test]
    fn audio_and_video_are_split_correctly() {
        assert!(!OutputFormat::Mov.is_audio());
        assert!(OutputFormat::Flac.is_audio());
    }
}
