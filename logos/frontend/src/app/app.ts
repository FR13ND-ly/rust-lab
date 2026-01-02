import { Component, inject } from '@angular/core';
import { Logos } from './logos';
import { Files } from './files/files';
import { Logs } from './logs/logs';
import { Stats } from './stats/stats';
import { Storages } from './storages/storages';

@Component({
  selector: 'app-root',
  imports: [Files, Logs, Stats, Storages],
  templateUrl: './app.html',
  styleUrl: './app.css'
})
export class App {
  service = inject(Logos);
}
