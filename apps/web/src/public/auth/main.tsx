import { createAuthRouter } from './router';
import { mountSurface } from '../surface';

mountSurface(createAuthRouter());
