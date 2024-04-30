use std::{error, fmt::Display};

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    filter, fmt,
    layer::{Layer, SubscriberExt},
    util::SubscriberInitExt,
    Registry,
};

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum Progress {
    #[cfg_attr(not(feature = "progress"), default)]
    Off,
    #[cfg(feature = "progress")]
    #[cfg_attr(feature = "progress", default)]
    ProgressBar,
}
impl Display for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            #[cfg(feature = "progress")]
            Self::ProgressBar => "progress-bar",
        })
    }
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum LogFormat {
    #[default]
    Full,
    Compact,
    Pretty,
    Json,
}
impl Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Full => "full",
            Self::Compact => "compact",
            Self::Pretty => "pretty",
            Self::Json => "json",
        })
    }
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
    Off,
}
impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Off => "off",
        })
    }
}

fn init<F>(f: F, env_filter: filter::EnvFilter, progress: Progress)
where
    F: fmt::FormatEvent<Registry, fmt::format::DefaultFields> + Send + Sync + 'static,
{
    match progress {
        #[cfg(feature = "progress")]
        Progress::ProgressBar => {
            use tracing_indicatif::IndicatifLayer;
            let indicatif_layer = IndicatifLayer::new()
                .with_max_progress_bars(u64::MAX, None)
                .with_span_child_prefix_indent("  ")
                .with_span_field_formatter(tracing_indicatif::filter::hide_indicatif_span_fields(
                    fmt::format::DefaultFields::new(),
                ));
            tracing_subscriber::registry()
                .with(
                    fmt::layer()
                        .with_writer(indicatif_layer.get_stdout_writer())
                        .event_format(f)
                        .with_filter(env_filter),
                )
                .with(
                    indicatif_layer
                        .with_filter(tracing_indicatif::filter::IndicatifFilter::new(false)),
                )
                .init()
        }
        Progress::Off => tracing_subscriber::registry()
            .with(fmt::layer().event_format(f).with_filter(env_filter))
            .init(),
    }
}

pub fn init_tracing(log_level: LogLevel, log_format: LogFormat, progress_bar: Progress) {
    use fmt::format;

    let env_filter = filter::EnvFilter::builder()
        .with_default_directive(filter::Directive::from(match log_level {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }))
        .from_env()
        .expect("valid log level environment");

    match log_format {
        LogFormat::Full => init(format::format(), env_filter, progress_bar),
        LogFormat::Compact => init(format::format().compact(), env_filter, progress_bar),
        LogFormat::Pretty => init(format::format().pretty(), env_filter, progress_bar),
        LogFormat::Json => init(format::format().pretty(), env_filter, progress_bar),
    }
}

/// traced error for printing and serializing
#[derive(Debug)]
pub struct ReportedErr<E>(pub E);
impl<E> From<E> for ReportedErr<E> {
    fn from(inner: E) -> Self {
        Self(inner)
    }
}
impl<E: error::Error> std::fmt::Display for ReportedErr<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        /// Wrapper type for indenting the inner source.
        struct Indented<'a, D> {
            inner: &'a mut D,
        }
        impl<T> Write for Indented<'_, T>
        where
            T: Write,
        {
            fn write_str(&mut self, s: &str) -> std::fmt::Result {
                for (i, line) in s.split('\n').enumerate() {
                    if i > 0 {
                        self.inner.write_char('\n')?;
                        self.inner.write_str("      ")?;
                    }

                    self.inner.write_str(line)?;
                }

                Ok(())
            }
        }

        write!(f, "{}", self.0)?;
        let mut cause = self.0.source();
        if cause.is_some() {
            write!(f, "\n\nCaused by:")?;
            let mut cnt = 0;
            while let Some(c) = cause {
                writeln!(f)?;
                let mut indented = Indented { inner: f };
                write!(indented, "{cnt: >4}: {c}")?;
                cause = c.source();
                cnt += 1;
            }
        }
        Ok(())
    }
}
impl<E: error::Error> error::Error for ReportedErr<E> {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        self.0.source()
    }
}
impl<E: error::Error> serde::Serialize for ReportedErr<E> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::{ser::SerializeSeq, Serialize};
        #[derive(Serialize)]
        struct Entry {
            description: String,
            debug: String,
        }
        let mut seq = serializer.serialize_seq(None)?;
        let mut err: Option<&dyn error::Error> = Some(&self.0);
        while let Some(e) = err {
            seq.serialize_element(&Entry {
                description: e.to_string(),
                debug: format!("{e:?}"),
            })?;
            err = e.source();
        }
        seq.end()
    }
}

#[cfg(feature = "progress")]
pub fn init_progress(message: &str, size: Option<usize>, span: &tracing::Span) {
    use tracing_indicatif::span_ext::IndicatifSpanExt;
    match size {
        Some(l) => {
            span.pb_set_length(l as u64);
            span.pb_set_style(
                &indicatif::ProgressStyle::with_template(
                    "{message} {pos}/{len} {percent}% {wide_bar}",
                )
                .unwrap(),
            );
        }
        None => {
            span.pb_set_length(0);
            span.pb_set_style(
                &indicatif::ProgressStyle::with_template("{spinner} {message} {pos}").unwrap(),
            )
        }
    }
    span.pb_set_message(message);
}
