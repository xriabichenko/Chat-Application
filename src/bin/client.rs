use iced::{
    widget::{button, column, row, text, text_input, container, scrollable, Button},
    alignment::Horizontal,
    Alignment, Element, Length, Application, Command, Settings, Theme, Subscription, Color, Background,
    theme,
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
use std::path::Path;
use chrono::{DateTime, Utc, Local};
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
    FetchUsers,
    FetchGroups,
    FetchMessages { target: ChatTarget },
    AddUserToGroup { group_id: Uuid, username: String },
    RemoveUserFromGroup { group_id: Uuid, username: String },
    DeleteGroup { group_id: Uuid },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum ServerResponse {
    Prompt(String),
    Success { message: String, data: Option<SuccessData> },
    Error(String),
    Message(Message),
    File(File),
    Users(Vec<User>),
    Groups(Vec<Group>),
    Messages(Vec<Message>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum SuccessData {
    Login { user_id: Uuid },
    GroupCreated { group_id: Uuid },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct User {
    id: Uuid,
    username: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Group {
    id: Uuid,
    name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum ChatTarget {
    User(Uuid),
    Group(Uuid),
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
    messages: Vec<Message>,
    status: String,
    client_state: Arc<Mutex<ClientState>>,
    writer: Arc<Mutex<Option<tokio::io::WriteHalf<TcpStream>>>>,
    reader: Arc<Mutex<Option<BufReader<tokio::io::ReadHalf<TcpStream>>>>>,
    crypto: Crypto,
    key: [u8; 32],
    view: View,
    users: Vec<User>,
    groups: Vec<Group>,
    current_chat: Option<ChatTarget>,
    add_user_input: String,
    remove_user_input: String,
    scroll_id: scrollable::Id,
}

#[derive(Default, Clone, PartialEq)]
enum View {
    #[default]
    Login,
    Main,
    Chat,
    GroupChat,
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
    AddUserInputChanged(String),
    RemoveUserInputChanged(String),
    Register,
    Login,
    SendMessage,
    CreateGroup,
    SendGroupMessage,
    SendFile,
    SendFileToGroup,
    AddUserToGroup,
    RemoveUserFromGroup,
    DeleteGroup,
    ServerResponse(ServerResponse),
    Connect,
    Error(String),
    FetchUsers,
    FetchGroups,
    SelectChat(ChatTarget),
    BackToMain,
    Logout,
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
                users: vec![],
                groups: vec![],
                current_chat: None,
                add_user_input: String::new(),
                remove_user_input: String::new(),
                scroll_id: scrollable::Id::new("chat_scroll"),
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
            AppMessage::AddUserInputChanged(username) => {
                self.add_user_input = username;
                Command::none()
            }
            AppMessage::RemoveUserInputChanged(username) => {
                self.remove_user_input = username;
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
                        Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Register sent".to_string())),
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
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Login sent".to_string())),
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
                let target = self.current_chat.clone().unwrap_or(ChatTarget::User(Uuid::nil()));
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
                Command::batch(vec![
                    Command::perform(
                        async move { send_message(writer, msg).await },
                        |result| match result {
                            Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Message sent".to_string())),
                            Err(e) => AppMessage::Error(e.to_string()),
                        },
                    ),
                    Command::perform(
                        async move { AppMessage::SelectChat(target) },
                        |msg| msg,
                    ),
                    scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 }),
                ])
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
                        Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Group creation sent".to_string())),
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
                let target = ChatTarget::Group(group_id);
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
                Command::batch(vec![
                    Command::perform(
                        async move { send_message(writer, msg).await },
                        |result| match result {
                            Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Group message sent".to_string())),
                            Err(e) => AppMessage::Error(e.to_string()),
                        },
                    ),
                    Command::perform(
                        async move { AppMessage::SelectChat(target) },
                        |msg| msg,
                    ),
                    scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 }),
                ])
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
                let target = self.current_chat.clone().unwrap_or(ChatTarget::User(Uuid::nil()));
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
                Command::batch(vec![
                    Command::perform(
                        async move { send_message(writer, msg).await },
                        |result| match result {
                            Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("File sent".to_string())),
                            Err(e) => AppMessage::Error(e.to_string()),
                        },
                    ),
                    Command::perform(
                        async move { AppMessage::SelectChat(target) },
                        |msg| msg,
                    ),
                    scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 }),
                ])
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
                let target = ChatTarget::Group(group_id);
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
                Command::batch(vec![
                    Command::perform(
                        async move { send_message(writer, msg).await },
                        |result| match result {
                            Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("File sent to group".to_string())),
                            Err(e) => AppMessage::Error(e.to_string()),
                        },
                    ),
                    Command::perform(
                        async move { AppMessage::SelectChat(target) },
                        |msg| msg,
                    ),
                    scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 }),
                ])
            }
            AppMessage::AddUserToGroup => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.group_id.is_empty() || self.add_user_input.is_empty() {
                    self.status = "Group ID and username required".to_string();
                    return Command::none();
                }
                let group_id = match Uuid::parse_str(&self.group_id) {
                    Ok(id) => id,
                    Err(_) => {
                        self.status = "Invalid group ID".to_string();
                        return Command::none();
                    }
                };
                let msg = ClientMessage::AddUserToGroup {
                    group_id,
                    username: self.add_user_input.clone(),
                };
                self.add_user_input.clear();
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Add user request sent".to_string())),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::RemoveUserFromGroup => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.group_id.is_empty() || self.remove_user_input.is_empty() {
                    self.status = "Group ID and username required".to_string();
                    return Command::none();
                }
                let group_id = match Uuid::parse_str(&self.group_id) {
                    Ok(id) => id,
                    Err(_) => {
                        self.status = "Invalid group ID".to_string();
                        return Command::none();
                    }
                };
                let msg = ClientMessage::RemoveUserFromGroup {
                    group_id,
                    username: self.remove_user_input.clone(),
                };
                self.remove_user_input.clear();
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, msg).await },
                    |result| match result {
                        Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Remove user request sent".to_string())),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::DeleteGroup => {
                if !self.client_state.blocking_lock().authenticated {
                    self.status = "Please login first".to_string();
                    return Command::none();
                }
                if self.group_id.is_empty() {
                    self.status = "Group ID required".to_string();
                    return Command::none();
                }
                let group_id = match Uuid::parse_str(&self.group_id) {
                    Ok(id) => id,
                    Err(_) => {
                        self.status = "Invalid group ID".to_string();
                        return Command::none();
                    }
                };
                let msg = ClientMessage::DeleteGroup { group_id };
                let writer = Arc::clone(&self.writer);
                Command::batch(vec![
                    Command::perform(
                        async move { send_message(writer, msg).await },
                        |result| match result {
                            Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Delete group request sent".to_string())),
                            Err(e) => AppMessage::Error(e.to_string()),
                        },
                    ),
                    Command::perform(
                        async { AppMessage::BackToMain },
                        |msg| msg,
                    ),
                ])
            }
            AppMessage::FetchUsers => {
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, ClientMessage::FetchUsers).await },
                    |result| match result {
                        Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Fetching users".to_string())),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::FetchGroups => {
                let writer = Arc::clone(&self.writer);
                Command::perform(
                    async move { send_message(writer, ClientMessage::FetchGroups).await },
                    |result| match result {
                        Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Fetching groups".to_string())),
                        Err(e) => AppMessage::Error(e.to_string()),
                    },
                )
            }
            AppMessage::SelectChat(target) => {
                self.current_chat = Some(target.clone());
                if let ChatTarget::User(user_id) = &target {
                    self.receiver_username = self.users
                        .iter()
                        .find(|user| user.id == *user_id)
                        .map(|user| user.username.clone())
                        .unwrap_or_default();
                    self.view = View::Chat;
                } else {
                    self.receiver_username.clear();
                    self.group_id = if let ChatTarget::Group(group_id) = &target {
                        group_id.to_string()
                    } else {
                        String::new()
                    };
                    self.view = View::GroupChat;
                }
                let writer = Arc::clone(&self.writer);
                Command::batch(vec![
                    Command::perform(
                        async move { send_message(writer, ClientMessage::FetchMessages { target }).await },
                        |result| match result {
                            Ok(()) => AppMessage::ServerResponse(ServerResponse::Prompt("Fetching messages".to_string())),
                            Err(e) => AppMessage::Error(e.to_string()),
                        },
                    ),
                    scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 }),
                ])
            }
            AppMessage::BackToMain => {
                self.view = View::Main;
                self.current_chat = None;
                self.messages.clear();
                self.receiver_username.clear();
                self.group_id.clear();
                self.add_user_input.clear();
                self.remove_user_input.clear();
                Command::none()
            }
            AppMessage::Logout => {
                let mut state = self.client_state.blocking_lock();
                state.authenticated = false;
                state.user_id = None;
                self.username.clear();
                self.public_key.clear();
                self.receiver_username.clear();
                self.group_name.clear();
                self.group_members.clear();
                self.group_id.clear();
                self.message_input.clear();
                self.file_path.clear();
                self.messages.clear();
                self.users.clear();
                self.groups.clear();
                self.current_chat = None;
                self.add_user_input.clear();
                self.remove_user_input.clear();
                self.view = View::Login;
                self.status = "Logged out".to_string();
                Command::none()
            }
            AppMessage::ServerResponse(response) => {
                match response {
                    ServerResponse::Prompt(msg) => {
                        self.status = msg.clone();
                        if msg.contains("Added to group") || msg.contains("Group deleted") {
                            return Command::perform(async { AppMessage::FetchGroups }, |msg| msg);
                        }
                    }
                    ServerResponse::Success { message, data } => {
                        self.status = format!("Success: {}", message);
                        if let Some(data) = data {
                            match data {
                                SuccessData::Login { user_id } => {
                                    let mut state = self.client_state.blocking_lock();
                                    state.authenticated = true;
                                    state.user_id = Some(user_id);
                                    self.view = View::Main;
                                    return Command::batch(vec![
                                        Command::perform(async { AppMessage::FetchUsers }, |msg| msg),
                                        Command::perform(async { AppMessage::FetchGroups }, |msg| msg),
                                    ]);
                                }
                                SuccessData::GroupCreated { group_id } => {
                                    self.status = format!("{} (group_id: {})", self.status, group_id);
                                    return Command::perform(async { AppMessage::FetchGroups }, |msg| msg);
                                }
                            }
                        }
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
                        let mut msg = msg.clone();
                        msg.content = String::from_utf8_lossy(&decrypted).to_string();
                        self.messages.push(msg);
                        return scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 });
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
                        let output_dir = "chat_downloads";
                        if let Err(e) = fs::create_dir_all(output_dir) {
                            self.status = format!("Failed to create directory {}: {}", output_dir, e);
                            return Command::none();
                        }
                        let output_path = format!("{}/received_{}", output_dir, file.filename);
                        if let Err(e) = fs::write(&output_path, decrypted) {
                            self.status = format!("Failed to save file to {}: {}", output_path, e);
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
                        let message = Message {
                            id: Uuid::new_v4(),
                            sender_id: file.sender_id,
                            receiver_id: file.receiver_id,
                            group_id: file.group_id,
                            content: display,
                            timestamp: file.timestamp,
                        };
                        self.messages.push(message);
                        return scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 });
                    }
                    ServerResponse::Users(users) => {
                        self.users = users;
                    }
                    ServerResponse::Groups(groups) => {
                        self.groups = groups;
                    }
                    ServerResponse::Messages(messages) => {
                        self.messages = messages
                            .into_iter()
                            .map(|mut msg| {
                                let decoded = match STANDARD.decode(&msg.content) {
                                    Ok(data) => data,
                                    Err(_) => return msg,
                                };
                                let decrypted = match self.crypto.decrypt(&self.key, &decoded) {
                                    Ok(data) => data,
                                    Err(_) => return msg,
                                };
                                msg.content = String::from_utf8_lossy(&decrypted).to_string();
                                msg
                            })
                            .collect();
                        return scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset { x: 0.0, y: 1.0 });
                    }
                }
                Command::none()
            }
            AppMessage::Connect => {
                let addr = self.server_addr.clone();
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
                    .width(Length::Fixed(300.0))
                    .style(theme::TextInput::Default);
                let public_key_input = text_input("Public Key", &self.public_key)
                    .on_input(AppMessage::PublicKeyChanged)
                    .padding(10)
                    .width(Length::Fixed(300.0))
                    .style(theme::TextInput::Default);

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
            View::Main => {
                let sidebar = scrollable(
                    column![
                        text("Users").size(20),
                        column(
                            self.users
                                .iter()
                                .filter(|user| {
                                    self.client_state
                                        .blocking_lock()
                                        .user_id
                                        .map_or(true, |self_id| self_id != user.id)
                                })
                                .map(|user| {
                                    Button::new(text(&user.username).size(16))
                                        .on_press(AppMessage::SelectChat(ChatTarget::User(user.id)))
                                        .padding(5)
                                        .width(Length::Fill)
                                        .into()
                                })
                                .collect::<Vec<_>>()
                        )
                        .spacing(5),
                        text("Groups").size(20),
                        column(
                            self.groups
                                .iter()
                                .map(|group| {
                                    Button::new(text(&group.name).size(16))
                                        .on_press(AppMessage::SelectChat(ChatTarget::Group(group.id)))
                                        .padding(5)
                                        .width(Length::Fill)
                                        .into()
                                })
                                .collect::<Vec<_>>()
                        )
                        .spacing(5),
                    ]
                        .spacing(10)
                        .padding(10),
                )
                    .width(Length::Fixed(200.0));

                let chat_area = column![text("Select a user or group to start chatting").size(20)];

                let group_creation = column![
                    text("Create Group").size(20),
                    text_input("Group Name", &self.group_name)
                        .on_input(AppMessage::GroupNameChanged)
                        .padding(10)
                        .width(Length::Fixed(200.0))
                        .style(theme::TextInput::Default),
                    text_input("Group Members (space-separated)", &self.group_members)
                        .on_input(AppMessage::GroupMembersChanged)
                        .padding(10)
                        .width(Length::Fixed(200.0))
                        .style(theme::TextInput::Default),
                    button("Create Group")
                        .on_press(AppMessage::CreateGroup)
                        .padding(10),
                ]
                    .spacing(10);

                let status = text(&self.status).size(16);
                let logout_button = button("Logout")
                    .on_press(AppMessage::Logout)
                    .padding(10);
                container(
                    column![
                        column![
                            text(format!("Logged in as: {}", self.username)).size(20),
                            row![text("Main").size(30), logout_button]
                                .spacing(20)
                                .align_items(Alignment::Center),
                        ]
                        .spacing(10),
                        row![
                            sidebar,
                            container(chat_area).padding(20).center_x().center_y(),
                            container(group_creation).padding(20)
                        ]
                        .spacing(20),
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
            View::Chat => {
                let back_button = button("Back to Main")
                    .on_press(AppMessage::BackToMain)
                    .padding(10);

                let message_display = scrollable(
                    column(
                        self.messages
                            .iter()
                            .map(|msg| {
                                let timestamp: DateTime<Utc> =
                                    DateTime::from_timestamp(msg.timestamp, 0).unwrap_or_default();
                                let is_sender = self.client_state
                                    .blocking_lock()
                                    .user_id
                                    .map_or(false, |self_id| self_id == msg.sender_id);

                                let message_row = row![
                                    text(&msg.content).size(16),
                                    text(format_timestamp(msg.timestamp))
                                        .size(12)
                                        .style(Color::from_rgb(0.5, 0.5, 0.5)),
                                ]
                                    .spacing(5)
                                    .align_items(Alignment::Center);

                                container(message_row)
                                    .padding(10)
                                    .width(Length::Shrink)
                                    .max_width(400)
                                    .style(move |_theme: &Theme| container::Appearance {
                                        background: Some(Background::Color(if is_sender {
                                            Color::from_rgb(0.2, 0.6, 1.0)
                                        } else {
                                            Color::from_rgb(1.0, 1.0, 1.0)
                                        })),
                                        border: iced::Border {
                                            color: Color::from_rgb(0.7, 0.7, 0.7),
                                            width: 1.0,
                                            radius: 8.0.into(),
                                        },
                                        ..Default::default()
                                    })
                                    .align_x(if is_sender {
                                        Horizontal::Right
                                    } else {
                                        Horizontal::Left
                                    })
                                    .into()
                            })
                            .collect::<Vec<_>>(),
                    )
                        .spacing(10)
                        .padding(10)
                        .width(Length::Fill),
                )
                    .height(Length::Fixed(400.0))
                    .id(self.scroll_id.clone());

                let message_input = text_input("Message", &self.message_input)
                    .on_input(AppMessage::MessageInputChanged)
                    .padding(10)
                    .width(Length::Fixed(400.0))
                    .style(theme::TextInput::Default);

                let send_button = button("Send Message")
                    .on_press(AppMessage::SendMessage)
                    .padding(10);

                let file_path_input = text_input("File Path", &self.file_path)
                    .on_input(AppMessage::FilePathChanged)
                    .padding(10)
                    .width(Length::Fixed(400.0))
                    .style(theme::TextInput::Default);

                let send_file_button = button("Send File")
                    .on_press(AppMessage::SendFile)
                    .padding(10);

                let status = text(&self.status).size(16);

                container(
                    column![
                        row![back_button].spacing(10),
                        text(format!("Chat: {}", self.receiver_username)).size(30),
                        message_display,
                        row![message_input, send_button].spacing(10),
                        row![file_path_input, send_file_button].spacing(10),
                        status
                    ]
                        .spacing(20)
                        .align_items(Alignment::Center)
                        .padding(20),
                )
                    .style(|_theme: &Theme| container::Appearance {
                        background: Some(Background::Color(Color::from_rgb(0.8, 0.8, 0.8))),
                        ..Default::default()
                    })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x()
                    .into()
            }
            View::GroupChat => {
                let back_button = button("Back to Main")
                    .on_press(AppMessage::BackToMain)
                    .padding(10);

                let group_name = self
                    .groups
                    .iter()
                    .find(|g| g.id == self.group_id.parse::<Uuid>().unwrap_or(Uuid::nil()))
                    .map(|g| g.name.clone())
                    .unwrap_or("Group".to_string());

                let message_display = scrollable(
                    column(
                        self.messages
                            .iter()
                            .map(|msg| {
                                let timestamp: DateTime<Utc> =
                                    DateTime::from_timestamp(msg.timestamp, 0).unwrap_or_default();
                                let is_sender = self.client_state
                                    .blocking_lock()
                                    .user_id
                                    .map_or(false, |self_id| self_id == msg.sender_id);
                                let sender_username = self
                                    .users
                                    .iter()
                                    .find(|user| user.id == msg.sender_id)
                                    .map(|user| user.username.clone())
                                    .unwrap_or("Unknown".to_string());

                                let message_row = row![
                                    text(format!("{}: {}", sender_username, &msg.content)).size(16),
                                    text(format_timestamp(msg.timestamp))
                                        .size(12)
                                        .style(Color::from_rgb(0.5, 0.5, 0.5)),
                                ]
                                    .spacing(5)
                                    .align_items(Alignment::Center);

                                container(message_row)
                                    .padding(10)
                                    .width(Length::Shrink)
                                    .max_width(400)
                                    .style(move |_theme: &Theme| container::Appearance {
                                        background: Some(Background::Color(if is_sender {
                                            Color::from_rgb(0.2, 0.6, 1.0)
                                        } else {
                                            Color::from_rgb(1.0, 1.0, 1.0)
                                        })),
                                        border: iced::Border {
                                            color: Color::from_rgb(0.7, 0.7, 0.7),
                                            width: 1.0,
                                            radius: 8.0.into(),
                                        },
                                        ..Default::default()
                                    })
                                    .align_x(if is_sender {
                                        Horizontal::Right
                                    } else {
                                        Horizontal::Left
                                    })
                                    .into()
                            })
                            .collect::<Vec<_>>(),
                    )
                        .spacing(10)
                        .padding(10)
                        .width(Length::Fill),
                )
                    .height(Length::Fixed(400.0))
                    .id(self.scroll_id.clone());

                let message_input = text_input("Message", &self.message_input)
                    .on_input(AppMessage::MessageInputChanged)
                    .padding(10)
                    .width(Length::Fixed(400.0))
                    .style(theme::TextInput::Default);

                let send_button = button("Send Group Message")
                    .on_press(AppMessage::SendGroupMessage)
                    .padding(10);

                let file_path_input = text_input("File Path", &self.file_path)
                    .on_input(AppMessage::FilePathChanged)
                    .padding(10)
                    .width(Length::Fixed(400.0))
                    .style(theme::TextInput::Default);

                let send_file_button = button("Send File to Group")
                    .on_press(AppMessage::SendFileToGroup)
                    .padding(10);

                let add_user_input = text_input("Add User (Username)", &self.add_user_input)
                    .on_input(AppMessage::AddUserInputChanged)
                    .padding(10)
                    .width(Length::Fixed(400.0))
                    .style(theme::TextInput::Default);

                let add_user_button = button("Add User to Group")
                    .on_press(AppMessage::AddUserToGroup)
                    .padding(10);

                let remove_user_input = text_input("Remove User (Username)", &self.remove_user_input)
                    .on_input(AppMessage::RemoveUserInputChanged)
                    .padding(10)
                    .width(Length::Fixed(400.0))
                    .style(theme::TextInput::Default);

                let remove_user_button = button("Remove User from Group")
                    .on_press(AppMessage::RemoveUserFromGroup)
                    .padding(10);

                let delete_group_button = button("Delete Group")
                    .on_press(AppMessage::DeleteGroup)
                    .padding(10);

                let status = text(&self.status).size(16);

                container(
                    column![
                        row![back_button, delete_group_button].spacing(10),
                        text(format!("Group Chat: {}", group_name)).size(30),
                        message_display,
                        row![message_input, send_button].spacing(10),
                        row![file_path_input, send_file_button].spacing(10),
                        row![add_user_input, add_user_button].spacing(10),
                        row![remove_user_input, remove_user_button].spacing(10),
                        status
                    ]
                        .spacing(20)
                        .align_items(Alignment::Center)
                        .padding(20),
                )
                    .style(|_theme: &Theme| container::Appearance {
                        background: Some(Background::Color(Color::from_rgb(0.8, 0.8, 0.8))),
                        ..Default::default()
                    })
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
            reader.clone(),
            move |reader_state| {
                let reader = Arc::clone(&reader);
                async move {
                    let mut reader_guard = reader.lock().await;
                    if let Some(reader) = reader_guard.as_mut() {
                        let mut line = String::new();
                        match reader.read_line(&mut line).await {
                            Ok(0) => (
                                AppMessage::Error("Server disconnected".to_string()),
                                reader_state,
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
                                reader_state,
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

fn format_timestamp(timestamp: i64) -> String {
    let datetime: DateTime<Utc> = DateTime::from_timestamp(timestamp, 0).unwrap_or_default();
    let local_datetime: DateTime<Local> = datetime.with_timezone(&Local);
    let now = Local::now();
    let today = now.date_naive();
    let message_date = local_datetime.date_naive();

    if message_date == today {
        local_datetime.format("%I:%M %p").to_string()
    } else if (today - message_date).num_days() == 1 {
        format!("Yesterday, {}", local_datetime.format("%I:%M %p"))
    } else {
        local_datetime.format("%b %d, %I:%M %p").to_string()
    }
}

fn main() -> iced::Result {
    ChatApp::run(Settings::default())
}