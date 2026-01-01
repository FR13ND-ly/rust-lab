import { TestBed } from '@angular/core/testing';

import { Logos } from './logos';

describe('Logos', () => {
  let service: Logos;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    service = TestBed.inject(Logos);
  });

  it('should be created', () => {
    expect(service).toBeTruthy();
  });
});
