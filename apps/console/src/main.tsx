import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';

import { App } from './app';
import './tokens.css';
import './app.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // A console is left open on a desk all day. Refetching on focus is the
      // behaviour that makes it feel live rather than stale, and the reads
      // here are small.
      staleTime: 30_000,
      // The client already retries a 401 once through the refresh path; a
      // second retry layer on top of that turns one expired token into four
      // requests. Everything else that fails is usually going to keep failing.
      retry: false,
    },
  },
});

const root = document.getElementById('root');
if (!root) throw new Error('#root is missing from index.html');

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
);
