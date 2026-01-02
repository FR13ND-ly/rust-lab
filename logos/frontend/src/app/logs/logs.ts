import { Component, ElementRef, inject, ViewChild } from '@angular/core';
import { Logos } from '../logos';
import { DatePipe, NgClass } from '@angular/common';

@Component({
  selector: 'app-logs',
  imports: [DatePipe, NgClass],
  templateUrl: './logs.html',
  styleUrl: './logs.css',
})
export class Logs {
  service = inject(Logos);
  
  @ViewChild('scrollContainer') private scrollContainer!: ElementRef;

  ngAfterViewChecked() {
    this.scrollToBottom();
  }

  scrollToBottom(): void {
    try {
      const el = this.scrollContainer.nativeElement;
      el.scrollTop = el.scrollHeight;
    } catch(err) { }
  }
}
