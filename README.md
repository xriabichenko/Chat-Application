# Chat-Application
FIIT RUST Project

Proposal: Chat Application

### **Names:** 
 Riabichenko Maksym,
 Rubchev Dmytro     

### **Introduction**

This project aims to develop a chat application using the Rust programming language. The application will consist of a server capable of handling multiple users simultaneously, allowing them to exchange text messages and files in real time. A key feature will be the implementation of group chats, enabling users to communicate within shared rooms. All messages will be stored persistently, ensuring data is not lost between sessions.

The primary problem this project addresses is providing a secure and efficient way for users to communicate and share files. Through this project, we aim to learn about network programming in Rust, including socket communication, concurrency handling, and data storage. 

### **Requirements**

#### To consider the project successful, the chat application should:

-Support multiple concurrent users connecting to the server.

-Allow users to send and receive real-time text messages.

-Enable file sharing between users.

-Provide group chat functionality where multiple users can interact simultaneously.

-Store chat messages and shared files persistently.

-Ensure secure and reliable communication between clients and the server.

### **Dependencies**

The following Rust crates will be considered to facilitate the development:

#### tokio: 
For asynchronous networking and concurrency.

#### serde:
For serializing and deserializing messages.

#### warp: 
For building the HTTP API if needed (e.g., user authentication).

#### rusqlite: 
For message and file metadata storage using SQLite.

#### uuid: 
For generating unique identifiers for users and messages.

### dotenv: 
For managing environment variables.

Other crates may be added as needed during development to address specific technical challenges.

