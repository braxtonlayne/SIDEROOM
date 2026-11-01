import './messages';
import { createGuestPlayRouter } from './router';
import { mountSurface } from '../surface';

mountSurface(createGuestPlayRouter());
