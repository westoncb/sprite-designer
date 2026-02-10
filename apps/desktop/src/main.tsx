import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

function App() {
  const [status, setStatus] = React.useState("Rust core not checked yet.");

  const checkBackend = async () => {
    try {
      const projects = await invoke("list_projects");
      setStatus(`Rust command bridge ready. Projects payload: ${JSON.stringify(projects)}`);
    } catch (error) {
      setStatus(`Command failed: ${String(error)}`);
    }
  };

  return (
    <main className="app">
      <h1>Sprite Designer</h1>
      <p>Rust-side core is scaffolded. UI implementation is intentionally deferred.</p>
      <button onClick={checkBackend}>Ping Rust Commands</button>
      <pre>{status}</pre>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
