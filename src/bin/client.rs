use iced::{
    widget::{button, column, row, text, text_input, container, scrollable},
    Alignment, Element, Length, Application, Command, Settings, Theme, Subscription,
};
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::fs;
use chrono::{DateTime, Utc};
use chat_application::models::{Message, File};
use chat_application::crypto::Crypto;

#[derive(Serialize, Deserialize, Clone)]
enum ClientMessage {
    Register { username: String, public_key: String },
    Login { username: String, public_key: String },
    Text(Message),
    Send { receiver_username: String, message: Message },
    CreateGroup { group_name: String, member_usernames: Vec<String> },
    SendToGroup { group_id: Uuid, message: Message },
    SendFile { receiver_username: String, file: File },
    SendFileToGroup { group_id: Uuid, file: File },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum ServerResponse {
    Prompt(String),
    SuccessLogin { message: String, user_id: Uuid },
    SuccessMSG(String),
    Error(String),
    Message(Message),
    File(File),
}

#[derive(Default)]
struct ClientState {
    authenticated: bool,
    user_id: Option<Uuid>,
}

struct ChatApp {
    server_addr: String,
    username: String,
    public_key: String,
    receiver_username: String,
    group_name: String,
    group_members: String,
    group_id: String,
    message_input: String,
    file_path: String,
    messages: Vec<String>,
    status: String,
    client_state: Arc<Mutex<ClientState>>,
    writer: Arc<Mutex<Option<tokio::io::WriteHalf<TcpStream>>>>,
    reader: Arc<Mutex<Option<BufReader<tokio::io::ReadHalf<TcpStream>>>>>,
    crypto: Crypto,
    key: [u8; 32],
    view: View,
}

#[derive(Default, Clone, PartialEq)]
enum View {
    #[default]
    Login,
    Chat,
}

#[derive(Clone, Debug)]
enum AppMessage {
    UsernameChanged(String),
    PublicKeyChanged(String),
    ReceiverUsernameChanged(String),
    GroupNameChanged(String),
    GroupMembersChanged(String),
    GroupIdChanged(String),
    MessageInputChanged(String),
    FilePathChanged(String),
    Register,
    Login,
    SendMessage,
    CreateGroup,
    SendGroupMessage,
    SendFile,
    SendFileToGroup,
    ServerResponse(ServerResponse),
    Connect,
    Error(String),
}

impl Application for ChatApp {
    type Executor = iced::executor::Default;
    type Message = AppMessage;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<AppMessage>) {
        (
            ChatApp {
                server_addr: "127.0.0.1:8080".to_string(),
                username: String::new(),
                public_key: String::new(),
                receiver_username: String::new(),
                group_name: String::new(),
                group_members: String::new(),
                group_id: String::new(),
                message_input: String::new(),
                file_path: String::new(),
                messages: vec![],
                status: "Disconnected".to_string(),
                client_state: Arc::new(Mutex::new(ClientState::default())),
                writer: Arc::new(Mutex::new(None)),
                reader: Arc::new(Mutex::new(None)),
                crypto: Crypto::new(),
                key: [0u8; 32],
                view: View::Login,
            },
            Command::perform(async { AppMessage::Connect }, |msg| msg),
        )
    }

    fn title(&self) -> String {
        String::from("Chat Application")
    }

    fn update(&mut self, message: AppMessage) -> Command<AppMessage> {
        match message {
            AppMessage::UsernameChanged(username) => {
                self.username = username;
                Command::none()
            }
            AppMessage::PublicKeyChanged(public_key) => {
                self.public_key = public_key;
                Command::none()
            }
            AppMessage::ReceiverUsernameChanged(receiver) => {
                self.receiver_username = receiver;
                Command::none()
            }
            AppMessage::GroupNameChanged(group_name) => {
                self.group_name = group_name;
                Command::none()
            }
            AppMessage::GroupMembersChanged(members) => {
                self.group_members = members;
                Command::none()
            }
            AppMessage::GroupIdChanged(group_id) => {
                self.group_id = group_id;
                Command::none()
            }
            AppMessage::MessageInputChanged(message) => {
                self.message_input = message;
                Command::none()
            }
            AppMessage::FilePathChanged(file_path) => {
                self.file_path = file_path;
                Command::none()
            }
            AppMessage::Register => {
                if self.username.is_empty() || self.public_key.is_empty() {
                    self.status = "Username and public key required".to_string();
                    return Command::none();
                }
                let msg = ClientMessage::Register {
                    username: self.username.clone(),
                    public_key: self.public_key.clone(),
                };
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::Error("Expected unit message".to_string()),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::Login => {
                if self.username.is_empty() || self.public_key.is_empty() {
                    self.status = "Username and public key required".to_string();
                    return Command::none();
                }
                let msg = ClientMessage::Login {
                    username: self.username.clone(),
                    public_key: self.public_key.clone(),
                };
                self.view = View::Chat;
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::Error("Expected unit message".to_string()),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::SendMessage => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.receiver_username.is_empty() || self.message_input.is_empty() {
                    self.status = "Receiver username and message required".to_string();
                    return Command::none();
                }
                let plaintext = self.message_input.as_bytes();
                let encrypted = self.crypto.encrypt(&self.key, plaintext).unwrap();
                let msg = ClientMessage::Send {
                    receiver_username: self.receiver_username.clone(),
                    message: Message {
                        id: Uuid::new_v4(),
                        sender_id: self.client_state.blocking_lock().user_id.unwrap(),
                        receiver_id: None,
                        group_id: None,
                        content: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                };
                self.message_input.clear();
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::Error("Expected unit message".to_string()),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::CreateGroup => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.group_name.is_empty() || self.group_members.is_empty() {
                    self.status = "Group name and members required".to_string();
                    return Command::none();
                }
                let members: Vec<String> = self.group_members
                    .split_whitespace()
                    .map(String::from)
                    .collect();
                let msg = ClientMessage::CreateGroup {
                    group_name: self.group_name.clone(),
                    member_usernames: members,
                };
                self.group_name.clear();
                self.group_members.clear();
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::Error("Expected unit message".to_string()),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::SendGroupMessage => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.group_id.is_empty() || self.message_input.is_empty() {
                    self.status = "Group ID and message required".to_string();
                    return Command::none();
                }
                let group_id = match Uuid::parse_str(&self.group_id) {
                    Ok(id) => id,
                    Err(_) => {
                        self.status = "Invalid group ID".to_string();
                        return Command::none();
                    }
                };
                let plaintext = self.message_input.as_bytes();
                let encrypted = self.crypto.encrypt(&self.key, plaintext).unwrap();
                let msg = ClientMessage::SendToGroup {
                    group_id,
                    message: Message {
                        id: Uuid::new_v4(),
                        sender_id: self.client_state.blocking_lock().user_id.unwrap(),
                        receiver_id: None,
                        group_id: Some(group_id),
                        content: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                };
                self.message_input.clear();
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::Error("Expected unit message".to_string()),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::SendFile => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.receiver_username.is_empty() || self.file_path.is_empty() {
                    self.status = "Receiver username and file path required".to_string();
                    return Command::none();
                }
                let file_path = normalize_path(&self.file_path);
                let file_data = match fs::read(&file_path) {
                    Ok(data) => data,
                    Err(e) => {
                        self.status = format!("Failed to read file: {}", e);
                        return Command::none();
                    }
                };
                let encrypted = self.crypto.encrypt(&self.key, &file_data).unwrap();
                let filename = std::path::Path::new(&file_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let msg = ClientMessage::SendFile {
                    receiver_username: self.receiver_username.clone(),
                    file: File {
                        id: Uuid::new_v4(),
                        sender_id: self.client_state.blocking_lock().user_id.unwrap(),
                        receiver_id: None,
                        group_id: None,
                        filename,
                        data: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                };
                self.file_path.clear();
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::Error("Expected unit message".to_string()),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::SendFileToGroup => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.group_id.is_empty() || self.file_path.is_empty() {
                    self.status = "Group ID and file path required".to_string();
                    return Command::none();
                }
                let group_id = match Uuid::parse_str(&self.group_id) {
                    Ok(id) => id,
                    Err(_) => {
                        self.status = "Invalid group ID".to_string();
                        return Command::none();
                    }
                };
                let file_path = normalize_path(&self.file_path);
                let file_data = match fs::read(&file_path) {
                    Ok(data) => data,
                    Err(e) => {
                        self.status = format!("Failed to read file: {}", e);
                        return Command::none();
                    }
                };
                let encrypted = self.crypto.encrypt(&self.key, &file_data).unwrap();
                let filename = std::path::Path::new(&file_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let msg = ClientMessage::SendFileToGroup {
                    group_id,
                    file: File {
                        id: Uuid::new_v4(),
                        sender_id: self.client_state.blocking_lock().user_id.unwrap(),
                        receiver_id: None,
                        group_id: Some(group_id),
                        filename,
                        data: STANDARD.encode(encrypted),
                        timestamp: chrono::Utc::now().timestamp(),
                    },
                };
                self.file_path.clear();
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::Error("Expected unit message".to_string()),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::ServerResponse(response) => {
                match response {
                    ServerResponse::Prompt(msg) => {
                        self.status = msg;
                    }
                    ServerResponse::SuccessLogin { message, user_id } => {
                        self.status = format!("Success: {} (user_id: {})", message, user_id);
                        let mut state = self.client_state.blocking_lock();
                        state.authenticated = true;
                        state.user_id = Some(user_id);
                        self.view = View::Chat;
                    }
                    ServerResponse::SuccessMSG(message) => {
                        self.status = format!("Success: {}", message);
                    }
                    ServerResponse::Error(msg) => {
                        self.status = format!("Error: {}", msg);
                    }
                    ServerResponse::Message(msg) => {
                        let decoded = match STANDARD.decode(&msg.content) {
                            Ok(data) => data,
                            Err(e) => {
                                self.status = format!("Base64 decode error: {}", e);
                                return Command::none();
                            }
                        };
                        let decrypted = match self.crypto.decrypt(&self.key, &decoded) {
                            Ok(data) => data,
                            Err(e) => {
                                self.status = format!("Decryption error: {}", e);
                                return Command::none();
                            }
                        };
                        let sender_info = if msg.group_id.is_some() {
                            format!("from group {}", msg.group_id.unwrap())
                        } else {
                            format!("from sender_id: {}", msg.sender_id)
                        };
                        let timestamp: DateTime<Utc> = DateTime::from_timestamp(msg.timestamp, 0)
                            .unwrap_or_default();
                        let display = format!(
                            "[{}] {}: {}",
                            timestamp.to_rfc3339(),
                            sender_info,
                            String::from_utf8_lossy(&decrypted)
                        );
                        self.messages.push(display);
                    }
                    ServerResponse::File(file) => {
                        let decoded = match STANDARD.decode(&file.data) {
                            Ok(data) => data,
                            Err(e) => {
                                self.status = format!("Base64 decode error: {}", e);
                                return Command::none();
                            }
                        };
                        let decrypted = match self.crypto.decrypt(&self.key, &decoded) {
                            Ok(data) => data,
                            Err(e) => {
                                self.status = format!("Decryption error: {}", e);
                                return Command::none();
                            }
                        };
                        let output_path = format!("received_{}", file.filename);
                        if let Err(e) = fs::write(&output_path, decrypted) {
                            self.status = format!("Failed to save file: {}", e);
                            return Command::none();
                        }
                        let sender_info = if file.group_id.is_some() {
                            format!("from group {}", file.group_id.unwrap())
                        } else {
                            format!(
                                "from sender_id: {} to receiver_id: {}",
                                file.sender_id,
                                file.receiver_id.map_or("unknown".to_string(), |id| id.to_string())
                            )
                        };
                        let timestamp: DateTime<Utc> = DateTime::from_timestamp(file.timestamp, 0)
                            .unwrap_or_default();
                        let display = format!(
                            "[{}] {}: File {} saved as {}",
                            timestamp.to_rfc3339(),
                            sender_info,
                            file.filename,
                            output_path
                        );
                        self.messages.push(display);
                    }
                }
                Command::none()
            }
            AppMessage::Connect => {
                let addr = self.server_addr.clone();
                let crypto = self.crypto.clone();
                let writer = Arc::clone(&self.writer);
                let reader = Arc::clone(&self.reader);
                Command::perform(
                    async move {
                        let stream = TcpStream::connect(&addr).await?;
                        let (read_half, write_half) = tokio::io::split(stream);
                        let buf_reader = BufReader::new(read_half);
                        *writer.lock().await = Some(write_half);
                        *reader.lock().await = Some(buf_reader);
                        Ok::<_, anyhow::Error>(())
                    },
                    |result| match result {
                        Ok(()) => {
                            AppMessage::ServerResponse(ServerResponse::Prompt(
                                "Connected".to_string(),
                            ))
                        }
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::Error(error) => {
                self.status = format!("Error: {}", error);
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<AppMessage> {
        match self.view {
            View::Login => {
                let username_input = text_input("Username", &self.username)
                    .on_input(AppMessage::UsernameChanged)
                    .padding(10)
                    .width(Length::Fixed(300.0));

                let public_key_input = text_input("Public Key", &self.public_key)
                    .on_input(AppMessage::PublicKeyChanged)
                    .padding(10)
                    .width(Length::Fixed(300.0));

                let register_button = button("Register")
                    .on_press(AppMessage::Register)
                    .padding(10);

                let login_button = button("Login")
                    .on_press(AppMessage::Login)
                    .padding(10);

                let status = text(&self.status).size(16);

                container(
                    column![
                    text("Chat Application").size(30),
                    username_input,
                    public_key_input,
                    row![register_button, login_button].spacing(10),
                    status
                ]
                        .spacing(20)
                        .align_items(Alignment::Center),
                )
                    .center_x()
                    .center_y()
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            View::Chat => {
                let message_display = scrollable(
                    column(
                        self.messages
                            .iter()
                            .map(|msg| text(msg).size(16).into())
                            .collect::<Vec<_>>(),
                    )
                        .spacing(10)
                        .padding(10),
                )
                    .height(Length::Fixed(300.0));

                let receiver_input = text_input("Receiver Username", &self.receiver_username)
                    .on_input(AppMessage::ReceiverUsernameChanged)
                    .padding(10)
                    .width(Length::Fixed(200.0));

                let message_input = text_input("Message", &self.message_input)
                    .on_input(AppMessage::MessageInputChanged)
                    .padding(10)
                    .width(Length::Fixed(200.0));

                let send_message_button = button("Send Message")
                    .on_press(AppMessage::SendMessage)
                    .padding(10);

                let group_name_input = text_input("Group Name", &self.group_name)
                    .on_input(AppMessage::GroupNameChanged)
                    .padding(10)
                    .width(Length::Fixed(200.0));

                let group_members_input = text_input(
                    "Group Members (space-separated)",
                    &self.group_members,
                )
                    .on_input(AppMessage::GroupMembersChanged)
                    .padding(10)
                    .width(Length::Fixed(200.0));

                let create_group_button = button("Create Group")
                    .on_press(AppMessage::CreateGroup)
                    .padding(10);

                let group_id_input = text_input("Group ID", &self.group_id)
                    .on_input(AppMessage::GroupIdChanged)
                    .padding(10)
                    .width(Length::Fixed(200.0));

                let group_message_input = text_input("Message", &self.message_input)
                    .on_input(AppMessage::MessageInputChanged)
                    .padding(10)
                    .width(Length::Fixed(200.0));

                let send_group_message_button = button("Send Group Message")
                    .on_press(AppMessage::SendGroupMessage)
                    .padding(10);

                let file_path_input = text_input("File Path", &self.file_path)
                    .on_input(AppMessage::FilePathChanged)
                    .padding(10)
                    .width(Length::Fixed(200.0));

                let send_file_button = button("Send File")
                    .on_press(AppMessage::SendFile)
                    .padding(10);

                let send_file_group_button = button("Send File to Group")
                    .on_press(AppMessage::SendFileToGroup)
                    .padding(10);

                let status = text(&self.status).size(16);

                container(
                    column![
                    text("Chat").size(30),
                    message_display,
                    row![receiver_input, message_input, send_message_button].spacing(10),
                    row![group_name_input, group_members_input, create_group_button].spacing(10),
                    row![group_id_input, group_message_input, send_group_message_button].spacing(10),
                    row![file_path_input, send_file_button, send_file_group_button].spacing(10),
                    status
                ]
                        .spacing(20)
                        .align_items(Alignment::Center)
                        .padding(20),
                )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x()
                    .into()
            }
        }
    }

    fn subscription(&self) -> Subscription<AppMessage> {
        struct ServerSubscription;

        let reader = Arc::clone(&self.reader);
        iced::subscription::unfold(
            std::any::TypeId::of::<ServerSubscription>(),
            reader.clone(), // Pass the Arc as initial state
            move |reader_state| {
                let reader = Arc::clone(&reader); // Clone Arc for async block
                async move {
                    let mut reader_guard = reader.lock().await;
                    if let Some(reader) = reader_guard.as_mut() {
                        let mut line = String::new();
                        match reader.read_line(&mut line).await {
                            Ok(0) => (
                                AppMessage::Error("Server disconnected".to_string()),
                                reader_state, // Return the Arc
                            ),
                            Ok(_) => {
                                if let Ok(response) = serde_json::from_str::<ServerResponse>(&line) {
                                    (AppMessage::ServerResponse(response), reader_state)
                                } else {
                                    (
                                        AppMessage::Error(format!("Invalid response: {}", line)),
                                        reader_state,
                                    )
                                }
                            }
                            Err(e) => (
                                AppMessage::Error(format!("Error reading: {}", e)),
                                reader_state, // Keep reader even on error to retry
                            ),
                        }
                    } else {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        (AppMessage::Error("No connection".to_string()), reader_state)
                    }
                }
            },
        )
    }
}

async fn send_message(
    writer: Arc<Mutex<Option<tokio::io::WriteHalf<TcpStream>>>>,
    msg: ClientMessage,
) -> Result<()> {
    let mut writer_guard = writer.lock().await;
    if let Some(writer) = writer_guard.as_mut() {
        let msg_json = serde_json::to_string(&msg)?;
        writer.write_all(msg_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("Not connected to server"))
    }
}

fn normalize_path(path: &str) -> String {
    path.replace("\\\\", "/").replace("\\", "/")
}

fn main() -> iced::Result {
    ChatApp::run(Settings::default())
}