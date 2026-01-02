import { Component, inject } from '@angular/core';
import { Logos } from '../logos';
import { SlicePipe } from '@angular/common';

@Component({
  selector: 'app-stats',
  imports: [SlicePipe],
  templateUrl: './stats.html',
  styleUrl: './stats.css',
})
export class Stats {
  service = inject(Logos);
  stats = this.service.stats;
}
