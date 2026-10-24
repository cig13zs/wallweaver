use std::io::Cursor;

use serde_json::json;
use ww_governor::{Policy, Reason, State};

use super::*;
use crate::transport::decode_incoming;

fn roundtrip<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(v: &T) {
    let mut buf = Vec::new();
    write_frame(&mut buf, v).unwrap();
    let back: T = read_frame(&mut Cursor::new(buf)).unwrap();
    assert_eq!(&back, v);
}

#[test]
fn every_request_round_trips() {
    let reqs = vec![
        Request::Status,
        Request::ListMonitors,
        Request::SetWallpaper {
            monitor: MonitorTarget::Id(1),
            source: WallpaperSource::File {
                path: "/pics/dunes.jpg".into(),
            },
        },
        Request::ClearWallpaper {
            monitor: MonitorTarget::All,
        },
        Request::PauseAll,
        Request::ResumeAll,
        Request::StopAll,
        Request::GetPolicy,
        Request::SetPolicy {
            policy: Policy::default(),
        },
        Request::SetProperties {
            monitor: MonitorTarget::All,
            values: json!({ "speed": 1.5 }).as_object().unwrap().clone(),
        },
        Request::GetConsent {
            source: WallpaperSource::Installed {
                id: "mira.loom".into(),
            },
        },
        Request::GrantPermissions {
            package_id: "mira.loom".into(),
            capabilities: vec![
                "audio".parse().unwrap(),
                "network:https://api.example.com".parse().unwrap(),
            ],
        },
        Request::RevokePermissions {
            package_id: "mira.loom".into(),
            capabilities: vec!["input.mouse".parse().unwrap()],
        },
        Request::Subscribe,
        Request::Shutdown,
    ];
    for (i, request) in reqs.into_iter().enumerate() {
        roundtrip(&ClientMessage::Request {
            id: i as u64,
            request,
        });
    }
}

#[test]
fn server_messages_round_trip() {
    let monitor = MonitorInfo {
        id: 0,
        name: "DP-1".into(),
        x: 0,
        y: 0,
        width: 2560,
        height: 1440,
        primary: true,
    };
    let msgs = vec![
        ServerMessage::Welcome {
            protocol: 1,
            daemon_version: "0.1.0".into(),
        },
        ServerMessage::Rejected {
            reason: "old".into(),
        },
        ServerMessage::Response {
            id: 3,
            result: Ok(Response::Ok),
        },
        ServerMessage::Response {
            id: 4,
            result: Err(RemoteError::new(ErrorCode::PermissionDenied, "no")),
        },
        ServerMessage::Response {
            id: 5,
            result: Ok(Response::Status(DaemonStatus {
                version: "0.1.0".into(),
                protocol: 1,
                uptime_ms: 10,
                rss_bytes: Some(9_000_000),
                adapter: None,
                monitors: vec![MonitorStatus {
                    monitor: monitor.clone(),
                    wallpaper: Some(WallpaperInfo {
                        id: "local.dunes".into(),
                        name: "dunes".into(),
                        kind: "image".into(),
                        source: WallpaperSource::File {
                            path: "/d.jpg".into(),
                        },
                    }),
                    state: Some(State::Paused),
                    reason: Some(Reason::FullscreenApp),
                }],
            })),
        },
        ServerMessage::Event {
            event: Event::StateChanged {
                monitor: 0,
                state: State::Running { fps: 60 },
                reason: Reason::Normal,
            },
        },
        ServerMessage::Event {
            event: Event::MonitorsChanged {
                monitors: vec![monitor],
            },
        },
    ];
    for m in &msgs {
        roundtrip(m);
    }
}

/// The wire format is a public contract with the UI. If this test needs
/// changing, the change is breaking for one of the two sides.
#[test]
fn wire_format_is_stable() {
    let hello = ClientMessage::Hello {
        min_protocol: 1,
        max_protocol: 1,
        client: "ui".into(),
    };
    assert_eq!(
        serde_json::to_value(&hello).unwrap(),
        json!({ "kind": "hello", "minProtocol": 1, "maxProtocol": 1, "client": "ui" })
    );
    let req = ClientMessage::Request {
        id: 7,
        request: Request::SetWallpaper {
            monitor: MonitorTarget::Id(2),
            source: WallpaperSource::Installed {
                id: "mira.loom".into(),
            },
        },
    };
    assert_eq!(
        serde_json::to_value(&req).unwrap(),
        json!({ "kind": "request", "id": 7, "request": {
            "type": "setWallpaper", "monitor": { "id": 2 }, "source": { "from": "installed", "id": "mira.loom" }
        }})
    );
    let all = serde_json::to_value(MonitorTarget::All).unwrap();
    assert_eq!(all, json!("all"));
    let ok = ServerMessage::Response {
        id: 7,
        result: Ok(Response::Ok),
    };
    assert_eq!(
        serde_json::to_value(&ok).unwrap(),
        json!({ "kind": "response", "id": 7, "result": { "Ok": { "type": "ok" } } })
    );
    let ev = ServerMessage::Event {
        event: Event::StateChanged {
            monitor: 1,
            state: State::Throttled { fps: 15 },
            reason: Reason::OnBattery,
        },
    };
    assert_eq!(
        serde_json::to_value(&ev).unwrap(),
        json!({ "kind": "event", "event": {
            "type": "stateChanged", "monitor": 1,
            "state": { "state": "throttled", "fps": 15 },
            "reason": { "reason": "onBattery" }
        }})
    );
}

#[test]
fn unknown_kinds_from_newer_daemons_decode_as_unknown() {
    let ev: ServerMessage = serde_json::from_value(json!({
        "kind": "event", "event": { "type": "audioLevels", "bins": [1, 2, 3] }
    }))
    .unwrap();
    assert_eq!(
        ev,
        ServerMessage::Event {
            event: Event::Unknown
        }
    );
    let resp: ServerMessage = serde_json::from_value(json!({
        "kind": "response", "id": 1, "result": { "Ok": { "type": "gallery", "items": [] } }
    }))
    .unwrap();
    assert_eq!(
        resp,
        ServerMessage::Response {
            id: 1,
            result: Ok(Response::Unknown)
        }
    );
    let code: ErrorCode = serde_json::from_value(json!("quotaExceeded")).unwrap();
    assert_eq!(code, ErrorCode::Unknown);
}

#[test]
fn unknown_requests_keep_their_id() {
    let bytes = serde_json::to_vec(&json!({
        "kind": "request", "id": 42, "request": { "type": "openGallery" }
    }))
    .unwrap();
    match decode_incoming(&bytes).unwrap() {
        Incoming::Unsupported { id, .. } => assert_eq!(id, 42),
        other => panic!("{other:?}"),
    }
    let bad = serde_json::to_vec(&json!({ "kind": "request", "request": {} })).unwrap();
    assert!(decode_incoming(&bad).is_err(), "no id");
    let hello = serde_json::to_vec(&json!({ "kind": "hello" })).unwrap();
    assert!(decode_incoming(&hello).is_err());
}

#[test]
fn negotiation() {
    assert_eq!(negotiate(1, 1), Some(1));
    assert_eq!(negotiate(1, 99), Some(PROTOCOL_VERSION));
    assert_eq!(negotiate(PROTOCOL_VERSION + 1, PROTOCOL_VERSION + 5), None);
    assert_eq!(negotiate(0, 0), None);
}

#[test]
fn oversized_and_truncated_frames_are_refused() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&((MAX_FRAME as u32) + 1).to_le_bytes());
    assert!(matches!(
        read_frame::<_, serde_json::Value>(&mut Cursor::new(buf)),
        Err(FrameError::TooLarge(_))
    ));
    let mut buf = Vec::new();
    buf.extend_from_slice(&10u32.to_le_bytes());
    buf.extend_from_slice(b"{}");
    assert!(matches!(
        read_frame::<_, serde_json::Value>(&mut Cursor::new(buf)),
        Err(FrameError::Io(_))
    ));
    assert!(matches!(
        read_frame::<_, serde_json::Value>(&mut Cursor::new(Vec::new())),
        Err(FrameError::Closed)
    ));
    assert!(matches!(
        read_frame::<_, serde_json::Value>(&mut Cursor::new(vec![1, 0])),
        Err(FrameError::Io(_))
    ));
    let big = "x".repeat(MAX_FRAME);
    assert!(matches!(
        write_frame(&mut Vec::new(), &big),
        Err(FrameError::TooLarge(_))
    ));
}

#[test]
fn consent_lines_convert() {
    let items = ww_permissions::consent_items(&[ww_permissions::Capability::ScriptEval]);
    let line: ConsentLine = items[0].clone().into();
    assert_eq!(line.risk, RiskLevel::High);
    assert_eq!(serde_json::to_value(line.risk).unwrap(), json!("high"));
}

#[cfg(unix)]
#[test]
fn client_and_server_talk_over_a_real_socket() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("run/daemon.sock"));
    let server = Server::bind(&endpoint).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("run"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "socket folder must be private");
    }
    let handle = std::thread::spawn(move || {
        let mut conn = server.accept().unwrap();
        assert_eq!(conn.handshake("9.9.9").unwrap(), PROTOCOL_VERSION);
        let mut events = conn.try_clone().unwrap();
        loop {
            match conn.recv() {
                Ok(Incoming::Request {
                    id,
                    request: Request::Status,
                }) => {
                    conn.respond(
                        id,
                        Ok(Response::Status(DaemonStatus {
                            version: "9.9.9".into(),
                            protocol: 1,
                            uptime_ms: 1,
                            rss_bytes: None,
                            adapter: None,
                            monitors: vec![],
                        })),
                    )
                    .unwrap();
                }
                Ok(Incoming::Request {
                    id,
                    request: Request::Subscribe,
                }) => {
                    events
                        .send_event(Event::StateChanged {
                            monitor: 0,
                            state: State::Paused,
                            reason: Reason::UserPaused,
                        })
                        .unwrap();
                    conn.respond(id, Ok(Response::Ok)).unwrap();
                }
                Ok(Incoming::Request {
                    id,
                    request: Request::Shutdown,
                }) => {
                    conn.respond(id, Ok(Response::Ok)).unwrap();
                    break;
                }
                Ok(Incoming::Request { id, .. }) => {
                    conn.respond(id, Err(RemoteError::new(ErrorCode::BadRequest, "nope")))
                        .unwrap();
                }
                Ok(Incoming::Unsupported { id, detail }) => {
                    conn.respond(id, Err(unsupported(&detail))).unwrap();
                }
                Err(e) => panic!("{e}"),
            }
        }
        server
    });

    let mut client = Client::connect(&endpoint, "test").unwrap();
    assert_eq!(client.daemon_version(), "9.9.9");
    assert_eq!(client.protocol(), PROTOCOL_VERSION);
    match client.request(Request::Status).unwrap() {
        Response::Status(s) => assert_eq!(s.version, "9.9.9"),
        other => panic!("{other:?}"),
    }
    // The event arrives before the response and is queued.
    assert_eq!(client.request(Request::Subscribe).unwrap(), Response::Ok);
    assert_eq!(
        client.next_event().unwrap(),
        Event::StateChanged {
            monitor: 0,
            state: State::Paused,
            reason: Reason::UserPaused
        }
    );
    match client.request(Request::PauseAll) {
        Err(IpcError::Remote(e)) => assert_eq!(e.code, ErrorCode::BadRequest),
        other => panic!("{other:?}"),
    }
    assert_eq!(client.request(Request::Shutdown).unwrap(), Response::Ok);
    let server = handle.join().unwrap();
    let path = match server.endpoint() {
        Endpoint::Path(p) => p.clone(),
        Endpoint::Pipe(_) => unreachable!(),
    };
    drop(server);
    assert!(!path.exists(), "socket is removed on shutdown");
}

#[cfg(unix)]
#[test]
fn refuses_a_shared_socket_folder() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared");
    std::fs::create_dir(&shared).unwrap();
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();
    let err = Server::bind(&Endpoint::Path(shared.join("daemon.sock"))).unwrap_err();
    assert!(matches!(err, IpcError::UnsafeSocketDir(_)), "{err}");
}

#[cfg(unix)]
#[test]
fn version_mismatch_is_rejected_with_a_reason() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("s/daemon.sock"));
    let server = Server::bind(&endpoint).unwrap();
    let handle = std::thread::spawn(move || {
        let mut conn = server.accept().unwrap();
        conn.handshake("1.0").unwrap_err()
    });
    let name = interprocess::local_socket::ToFsName::to_fs_name::<
        interprocess::local_socket::GenericFilePath,
    >(match &endpoint {
        Endpoint::Path(p) => p.as_path(),
        Endpoint::Pipe(_) => unreachable!(),
    })
    .unwrap();
    let mut stream = <interprocess::local_socket::Stream as interprocess::local_socket::traits::Stream>::connect(name).unwrap();
    write_frame(
        &mut stream,
        &ClientMessage::Hello {
            min_protocol: 50,
            max_protocol: 60,
            client: "future".into(),
        },
    )
    .unwrap();
    let reply: ServerMessage = read_frame(&mut stream).unwrap();
    assert!(
        matches!(reply, ServerMessage::Rejected { ref reason } if reason.contains("update")),
        "{reply:?}"
    );
    assert!(matches!(handle.join().unwrap(), IpcError::Rejected(_)));
}

#[test]
fn default_endpoint_is_per_user() {
    let e = Endpoint::default_for_user();
    let s = e.to_string();
    assert!(s.contains("wallweaver"), "{s}");
}
