import { Injectable, signal } from '@angular/core';

export interface StorageInfo {
  id: string;
  name: string;
}

export interface FileMetadata {
  path: string;
  size: number;
  modified: number;
  version: number;
  hash: string;
  is_deleted: boolean;
  last_modified_by?: string;
}

export interface ActivityEntry {
  id: string;
  type: 'file-update' | 'file-delete' | 'system' | 'error' | 'connect';
  message: string;
  user?: string;
  timestamp: number;
}

export interface ClientInfo {
  id: string;
  name: string;
  storage_id: string;
}

export interface Stats {
  active_clients: number;
  total_files: number;
  client_details: ClientInfo[];
}

@Injectable({
  providedIn: 'root',
})
export class Logos {
  private socket: WebSocket | null = null;
  
  private downloadQueue = new Set<string>();
  private nextBinaryMetadata: { path: string } | null = null;

  readonly WEBSOCKET_URL = 'ws://localhost:3000/ws/client';

  storages = signal<StorageInfo[]>([]);
  activeStorageId = signal<string | null>(null);
  files = signal<FileMetadata[]>([]);
  activity = signal<ActivityEntry[]>([]); 
  stats = signal<Stats>({ active_clients: 0, total_files: 0, client_details: [] });
  isConnected = signal<boolean>(false);

  constructor() {
    this.connect();
  }

  connect() {
    this.socket = new WebSocket(this.WEBSOCKET_URL);

    this.socket.onopen = () => {
      this.isConnected.set(true);
      this.addActivity('connect', 'Dashboard connected', 'System');
      this.send('RegisterDashboard');
      this.send('RequestStorageList');
    };

    this.socket.onmessage = (event) => {
      if (event.data instanceof Blob) {
        this.handleBinary(event.data);
        return;
      }
      
      try {
        const msg = JSON.parse(event.data);
        this.handleMessage(msg);
      } catch (e) {
        console.error('Failed to parse message', e);
      }
    };

    this.socket.onclose = () => {
      this.isConnected.set(false);
      this.addActivity('error', 'Connection lost', 'System');
      setTimeout(() => this.connect(), 3000);
    };
  }

  private handleMessage(msg: any) {
    if (msg.StorageList) {
      this.storages.set(msg.StorageList.storages);
    } 
    else if (msg.Welcome) {
      const activeFiles = msg.Welcome.files.filter((f: FileMetadata) => !f.is_deleted);
      this.files.set(activeFiles);
      this.addActivity('system', `Joined storage: ${msg.Welcome.storage_id.substring(0, 8)}...`, 'System');
    }
    else if (msg.StartTransfer) {
      const path = msg.StartTransfer.path;
      if (this.downloadQueue.has(path)) {
        this.nextBinaryMetadata = { path };
        this.addActivity('system', `Downloading ${path}...`, 'System');
      } else {
        this.nextBinaryMetadata = null;
      }
    }
    else if (msg.Log) {
      if (!msg.Log.message.includes('File updated in')) {
         const type = msg.Log.level === 'error' ? 'error' : 'system';
         this.addActivity(type, msg.Log.message, 'Server', msg.Log.timestamp);
      }
    }
    else if (msg.Stats) {
      this.stats.set(msg.Stats);
    }
    else if (msg.FileUpdate) {
      const meta = msg.FileUpdate.meta;
      this.files.update(curr => {
        const exists = curr.findIndex(f => f.path === meta.path);
        if (exists > -1) {
          const next = [...curr];
          next[exists] = meta;
          return next;
        }
        return [...curr, meta];
      });

      this.addActivity(
        'file-update', 
        meta.path, 
        meta.last_modified_by || 'Unknown', 
        meta.modified
      );
    }
    else if (msg.DeleteFile) {
        const path = msg.DeleteFile.path;
        this.files.update(curr => curr.map(f => 
            f.path === path ? { ...f, is_deleted: true } : f
        ));
        this.addActivity('file-delete', path, 'Unknown');
    }
  }

  downloadFile(path: string) {
    if (!this.isConnected()) return;
    this.downloadQueue.add(path);
    this.send({ RequestFile: { path } });
  }

  deleteFile(path: string) {
    if (!this.isConnected()) return;
    if(confirm(`Are you sure you want to delete '${path}'?\nThis will remove it for all connected clients.`)) {
        this.files.update(curr => curr.map(f => 
            f.path === path ? { ...f, is_deleted: true } : f
        ));
        this.send({ DeleteFile: { path } });
    }
  }

  private handleBinary(blob: Blob) {
    if (this.nextBinaryMetadata) {
      const path = this.nextBinaryMetadata.path;
      this.triggerBrowserDownload(blob, path);
      
      this.downloadQueue.delete(path);
      this.nextBinaryMetadata = null;
    }
  }

  private triggerBrowserDownload(blob: Blob, filename: string) {
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    window.URL.revokeObjectURL(url);
    document.body.removeChild(a);
  }

  private addActivity(type: ActivityEntry['type'], message: string, user: string = 'System', timestamp?: number) {
    const entry: ActivityEntry = {
      id: Math.random().toString(36).substring(7),
      type,
      message,
      user,
      timestamp: timestamp || Date.now() / 1000
    };
    this.activity.update(prev => [...prev, entry].slice(-100)); 
  }

  send(msg: any) {
    if (this.socket && this.socket.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(msg));
    }
  }

  joinStorage(id: string) {
    this.activeStorageId.set(id);
    this.files.set([]); 
    this.send({ JoinStorage: { storage_id: id, client_name: 'Dashboard' } });
  }

  refreshStorages() {
    this.send('RequestStorageList');
  }

  createStorage(name: string) {
    this.send({ CreateStorage: { name } });
  }

  deleteStorage(id: string) {
    this.storages.update(list => list.filter(s => s.id !== id));
    if (this.activeStorageId() === id) {
        this.activeStorageId.set(null);
        this.files.set([]);
    }
    this.send({ DeleteStorage: { storage_id: id } });
  }
}
