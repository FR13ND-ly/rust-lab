import { Component, computed, inject, signal } from '@angular/core';
import { FileMetadata, Logos } from '../logos';
import { DatePipe } from '@angular/common';
import { FormsModule } from '@angular/forms';

@Component({
  selector: 'app-files',
  imports: [DatePipe, FormsModule],
  templateUrl: './files.html',
  styleUrl: './files.css',
})
export class Files {
  service = inject(Logos);
  
  searchQuery = signal('');
  sortColumn = signal<keyof FileMetadata>('path');
  sortDirection = signal<'asc' | 'desc'>('asc');

  activeStorageName = computed(() => {
    const id = this.service.activeStorageId();
    return this.service.storages().find(s => s.id === id)?.name;
  });

  sortedFiles = computed(() => {
    let files = this.service.files();
    
    const q = this.searchQuery().toLowerCase();
    if (q) {
        files = files.filter(f => 
            f.path.toLowerCase().includes(q) || 
            (f.last_modified_by?.toLowerCase().includes(q))
        );
    }

    const col = this.sortColumn();
    const dir = this.sortDirection() === 'asc' ? 1 : -1;

    return files.sort((a, b) => {
        let valA: any = a[col];
        let valB: any = b[col];

        if (valA === undefined) valA = '';
        if (valB === undefined) valB = '';
        if (typeof valA === 'string') valA = valA.toLowerCase();
        if (typeof valB === 'string') valB = valB.toLowerCase();
        
        if (valA < valB) return -1 * dir;
        if (valA > valB) return 1 * dir;
        return 0;
    });
  });

  sort(column: keyof FileMetadata) {
      if (this.sortColumn() === column) {
          this.sortDirection.set(this.sortDirection() === 'asc' ? 'desc' : 'asc');
      } else {
          this.sortColumn.set(column);
          this.sortDirection.set('asc');
      }
  }

  formatSize(bytes: number) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  getFileIcon(path: string) {
      const ext = path.split('.').pop()?.toLowerCase();
      let icon = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-blue-400"><path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/><polyline points="14 2 14 8 20 8"/></svg>`;
      
      if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(ext || '')) {
          icon = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-purple-400"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>`;
      } else if (['js', 'ts', 'rs', 'html', 'css', 'json', 'py', 'cpp'].includes(ext || '')) {
          icon = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-yellow-500"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>`;
      } else if (['zip', 'rar', '7z', 'tar', 'gz'].includes(ext || '')) {
          icon = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-orange-400"><path d="M21 8v13H3V8"/><path d="M1 3h22v5H1z"/><path d="M10 12h4"/></svg>`;
      }
      return this.sanitizer(icon);
  }

  private sanitizer(html: string) { return html; }
}
