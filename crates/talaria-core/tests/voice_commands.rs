//! `/voice` forms and the local command table.

use talaria_core::commands::{LocalAction, LocalCommandTable};
use talaria_core::voice::{VoiceCommand, command_outbound, parse_voice_args};

#[test]
fn voice_command_forms() {
    let table = LocalCommandTable::builtin();
    let Some(command) = table.find("voice") else {
        panic!("voice is not in the local command table");
    };
    assert!(command.takes_argument, "voice accepts an argument");
    assert_eq!(command.action, LocalAction::Voice);
    assert!(
        table.find("VOICE").is_some(),
        "the name is case-insensitive"
    );

    let forms = [
        ("", VoiceCommand::Status),
        ("status", VoiceCommand::Status),
        ("STATUS", VoiceCommand::Status),
        ("on", VoiceCommand::On),
        ("ON", VoiceCommand::On),
        ("off", VoiceCommand::Off),
        ("  Off  ", VoiceCommand::Off),
        ("read", VoiceCommand::Read(None)),
        ("tts", VoiceCommand::Read(None)),
        ("TTS", VoiceCommand::Read(None)),
        ("read on", VoiceCommand::Read(Some(true))),
        ("Read On", VoiceCommand::Read(Some(true))),
        ("read off", VoiceCommand::Read(Some(false))),
        ("  read   off ", VoiceCommand::Read(Some(false))),
    ];
    for (arg, expected) in forms {
        assert_eq!(
            parse_voice_args(arg).as_ref().ok(),
            Some(&expected),
            "{arg}"
        );
    }

    let banana = parse_voice_args("banana").expect_err("banana");
    assert_eq!(
        banana,
        "Unknown /voice option \"banana\". Use /voice on, off, status, read on or read off."
    );
    let tts_on = parse_voice_args("tts on").expect_err("tts on");
    assert_eq!(
        tts_on,
        "Unknown /voice option \"tts on\". Use /voice on, off, status, read on or read off."
    );

    assert!(command_outbound(&VoiceCommand::Read(None)).is_empty());
    assert!(command_outbound(&VoiceCommand::Read(Some(true))).is_empty());
    assert!(command_outbound(&VoiceCommand::Read(Some(false))).is_empty());
    assert!(!command_outbound(&VoiceCommand::On).is_empty());
    assert!(!command_outbound(&VoiceCommand::Status).is_empty());
}
