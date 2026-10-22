# QuickLendX Frontend

[![Next.js](https://img.shields.io/badge/Next.js-15.4-000000?style=for-the-badge&logo=next.js&logoColor=white)](https://nextjs.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0-3178C6?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Tailwind CSS](https://img.shields.io/badge/Tailwind-4.0-38B2AC?style=for-the-badge&logo=tailwind-css&logoColor=white)](https://tailwindcss.com/)

Modern, production-ready frontend application for the QuickLendX invoice financing protocol. Built with Next.js 15, TypeScript, and Tailwind CSS, providing a seamless user experience for businesses and investors.

> **Note**: This is the frontend application. For the full project documentation, see the [main README](../README.md).

## 🎯 Overview

The QuickLendX frontend provides a comprehensive interface for:

- **Businesses**: Upload invoices, manage KYC verification, track invoice status, and receive funding
- **Investors**: Browse available invoices, place bids, manage investments, and track returns
- **Administrators**: Monitor platform metrics, manage verifications, and handle disputes

## 🚀 Quick Start

### Prerequisites

- **Node.js** 18+ ([Download](https://nodejs.org/))
- **npm** or **yarn** package manager
- Access to deployed QuickLendX smart contracts

### Installation

1. **Navigate to frontend directory**

```bash
cd quicklendx-frontend
```

2. **Install dependencies**

```bash
npm install
# or
yarn install
```

3. **Set up environment variables**

Create a `.env.local` file in the `quicklendx-frontend` directory:

```bash
# Contract Configuration
NEXT_PUBLIC_CONTRACT_ID=your_contract_id_here
NEXT_PUBLIC_NETWORK=testnet

# RPC Configuration
NEXT_PUBLIC_RPC_URL=https://soroban-testnet.stellar.org:443

# Optional: Analytics
NEXT_PUBLIC_ANALYTICS_ID=your_analytics_id

# API base URL (must include the full path prefix, e.g. /api/v1).
# Defaults to http://localhost:3001/api/v1 when unset.
# The backend mounts all v1 routes at /api/v1 (backend/src/app.ts).
NEXT_PUBLIC_API_URL=http://localhost:3001/api/v1
```

4. **Run development server**

```bash
npm run dev
# or
yarn dev
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

## 📁 Project Structure

```
quicklendx-frontend/
├── app/                      # Next.js App Router
│   ├── components/           # React components
│   │   ├── ClientOnly.tsx   # Client-side only wrapper
│   │   ├── ErrorBoundary.tsx # Error boundary component
│   │   └── ErrorToast.tsx    # Error notification component
│   ├── lib/                  # Utility libraries
│   │   ├── api-client.ts     # Contract API client
│   │   ├── errors.ts         # Error handling
│   │   └── validation.ts     # Input validation
│   ├── globals.css           # Global styles
│   ├── layout.tsx            # Root layout
│   └── page.tsx              # Home page
├── public/                   # Static assets
│   └── quicklendx.png        # Brand assets
├── next.config.ts            # Next.js configuration
├── package.json              # Dependencies
├── postcss.config.mjs        # PostCSS configuration
└── tsconfig.json             # TypeScript configuration
```

## 🛠️ Development

### Available Scripts

- `npm run dev` - Start development server with Turbopack
- `npm run build` - Build for production
- `npm run start` - Start production server
- `npm run lint` - Run ESLint
- `npm test` - Run Jest Node and jsdom projects
- `npm run test:e2e` - Build and run Playwright browser tests

### Code Organization

- **Components**: Reusable UI components in `app/components/`
- **API Client**: Contract interaction logic in `app/lib/api-client.ts`
- **Validation**: Input validation schemas in `app/lib/validation.ts`
- **Error Handling**: Centralized error handling in `app/lib/errors.ts`

### Styling

The project uses **Tailwind CSS 4.0** for styling. Global styles and theme configuration are in `app/globals.css`.

## 🔧 Configuration

### Next.js Configuration

The `next.config.ts` file contains Next.js configuration. Key settings:

- React strict mode enabled
- Image optimization configured
- TypeScript strict mode

### Environment Variables

Required environment variables:

- `NEXT_PUBLIC_CONTRACT_ID`: Deployed contract ID
- `NEXT_PUBLIC_NETWORK`: Network (testnet/mainnet)
- `NEXT_PUBLIC_RPC_URL`: Soroban RPC endpoint

Optional:

- `NEXT_PUBLIC_ANALYTICS_ID`: Analytics tracking ID
- `NEXT_PUBLIC_API_URL`: Backend API base URL **including the `/api/v1` prefix**
  (e.g. `http://localhost:3001/api/v1`). Defaults to `http://localhost:3001/api/v1` when
  unset. Matches the v1 router mount in `backend/src/app.ts` and the server entry in
  `backend/openapi.yaml`.

## 🧪 Testing

### Run Linter

```bash
npm run lint
```

### Build for Production

```bash
npm run build
```

This will:

- Type-check the codebase
- Build optimized production bundle
- Generate static assets

### UI and End-to-End Tests

```bash
npm test
npx playwright install chromium
npm run test:e2e
```

Jest runs the existing `*.test.ts` suites in Node and `*.test.tsx` component
tests in jsdom. Playwright starts a production build with the test-only wallet
injection enabled, then captures responsive settlement-list states and runs an
axe accessibility check. The wallet injection is guarded by
`NEXT_PUBLIC_E2E=1` and is not enabled in normal production builds.

## 🚀 Deployment

### Vercel (Recommended)

1. Push your code to GitHub
2. Import project in [Vercel](https://vercel.com)
3. Configure environment variables
4. Deploy

### Other Platforms

The application can be deployed to any platform supporting Next.js:

- **Netlify**: Use Next.js build preset
- **AWS Amplify**: Configure build settings
- **Docker**: Use provided Dockerfile (if available)

### Production Checklist

- [ ] Environment variables configured
- [ ] Contract addresses updated
- [ ] Analytics configured (if used)
- [ ] Error tracking set up
- [ ] Performance optimization verified
- [ ] SEO metadata configured
- [ ] Security headers configured

## 📦 Dependencies

### Core Dependencies

- **next** (15.4.1): React framework
- **react** (19.1.0): UI library
- **react-dom** (19.1.0): React DOM bindings
- **typescript** (5.0): Type safety

### UI & Styling

- **tailwindcss** (4.0): Utility-first CSS framework
- **@tailwindcss/postcss** (4.0): PostCSS plugin

### Utilities

- **axios** (1.6.0): HTTP client
- **zod** (3.22.4): Schema validation
- **react-hot-toast** (2.4.1): Toast notifications
- **react-error-boundary** (4.0.12): Error handling

## 🔗 Integration with Smart Contracts

The frontend interacts with QuickLendX smart contracts through:

1. **API Client** (`app/lib/api-client.ts`): Wraps contract function calls
2. **Validation** (`app/lib/validation.ts`): Validates inputs before submission
3. **Error Handling** (`app/lib/errors.ts`): Handles contract errors gracefully

### Example Usage

```typescript
import { apiClient } from "@/lib/api-client";

// Get invoice details
const invoice = await apiClient.getInvoice(invoiceId);

// Place a bid
await apiClient.placeBid({
  investor: address,
  invoiceId: invoiceId,
  bidAmount: amount,
  expectedReturn: returnAmount,
});
```

## 🎨 UI Components

### Available Components

- **ClientOnly**: Wraps components that require client-side rendering
- **ErrorBoundary**: Catches and displays React errors
- **ErrorToast**: Displays error notifications

### Component Guidelines

- Use TypeScript for all components
- Follow Next.js App Router conventions
- Implement proper error handling
- Ensure accessibility (a11y) compliance
- Optimize for performance

## 🔒 Security Considerations

- Never expose private keys in client-side code
- Validate all user inputs
- Sanitize data before rendering
- Use HTTPS in production
- Implement proper CORS policies
- Rate limit API calls

## 📚 Resources

- [Next.js Documentation](https://nextjs.org/docs)
- [TypeScript Documentation](https://www.typescriptlang.org/docs/)
- [Tailwind CSS Documentation](https://tailwindcss.com/docs)
- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)

## 🤝 Contributing

See the [main Contributing Guide](../quicklendx-contracts/CONTRIBUTING.md) for contribution guidelines.

### Frontend-Specific Guidelines

- Follow TypeScript best practices
- Use functional components with hooks
- Implement proper error boundaries
- Write accessible components
- Optimize bundle size
- Test in multiple browsers

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](../LICENSE) file for details.

---

**Built with Next.js and Stellar Soroban**
