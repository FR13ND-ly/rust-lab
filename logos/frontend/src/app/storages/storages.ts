import { Component, inject } from '@angular/core';
import { Logos } from '../logos';
import { FormsModule } from '@angular/forms';

@Component({
  selector: 'app-storages',
  imports: [FormsModule],
  templateUrl: './storages.html',
  styleUrl: './storages.css',
})
export class Storages {
  service = inject(Logos);
  newStorageName = '';

  create() {
    if (!this.newStorageName.trim()) return;
    this.service.createStorage(this.newStorageName);
    this.newStorageName = '';
  }

  delete(id: string) {
    if(confirm('Are you sure you want to delete this storage? All files will be lost.')) {
        this.service.deleteStorage(id);
    }
  }

  copyId(id: string) {
      navigator.clipboard.writeText(id);
  }
}
