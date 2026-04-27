/**
 * QuickLendX Protocol - Local Development Seed Script
 * Populates Postgres with sample data for local development.
 */

import { createId } from '@paralleldrive/cuid2';

/**
 * NOTE: The following types and MockPrismaClient are placeholders for development.
 * In a production-ready implementation, replace the mock with a real ORM client.
 */

interface User {
  id: string;
  email: string;
  name: string;
  role: 'BUSINESS' | 'INVESTOR';
  isVerified: boolean;
  taxId?: string;
}

interface Invoice {
  id: string;
  ownerId: string;
  amount: number;
  currency: string;
  status: 'PENDING' | 'VERIFIED' | 'FUNDED' | 'PAID' | 'CANCELLED';
  description: string;
  dueDate: Date;
  category: 'SERVICES' | 'GOODS';
}

interface Bid {
  id: string;
  investorId: string;
  invoiceId: string;
  bidAmount: number;
  expectedReturn: number;
  status: 'PLACED' | 'ACCEPTED' | 'REJECTED' | 'CANCELLED';
  createdAt: Date;
}

class MockPrismaClient {
  user = {
    deleteMany: async () => ({ count: 0 }),
    createMany: async (args: { data: User[] }) => ({ count: args.data.length }),
  };
  invoice = {
    deleteMany: async () => ({ count: 0 }),
    createMany: async (args: { data: Invoice[] }) => ({ count: args.data.length }),
  };
  bid = {
    deleteMany: async () => ({ count: 0 }),
    createMany: async (args: { data: Bid[] }) => ({ count: args.data.length }),
  };
  $disconnect = async () => {};
}

const db = new MockPrismaClient();

async function main() {
  console.log('🌱 Starting database seed...');

  if (process.env.NODE_ENV === 'production') {
    console.error('❌ Error: Seed script cannot be run in production environment.');
    process.exit(1);
  }

  console.log('🧹 Clearing existing data...');
  await db.bid.deleteMany();
  await db.invoice.deleteMany();
  await db.user.deleteMany();

  console.log('👤 Creating sample users...');
  const businessUser: User = {
    id: createId(),
    email: 'business@example.com',
    name: 'Acme Services Corp',
    role: 'BUSINESS',
    isVerified: true,
    taxId: 'TX-998877',
  };

  const investorUser: User = {
    id: createId(),
    email: 'investor@example.com',
    name: 'Stellar Capital',
    role: 'INVESTOR',
    isVerified: true,
  };

  await db.user.createMany({ data: [businessUser, investorUser] });

  console.log('📄 Creating sample invoices...');
  const invoices: Invoice[] = [
    {
      id: createId(),
      ownerId: businessUser.id,
      amount: 500000,
      currency: 'USDC',
      status: 'VERIFIED',
      description: 'Q3 Software Consulting Services',
      dueDate: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000),
      category: 'SERVICES',
    }
  ];

  await db.invoice.createMany({ data: invoices });

  console.log('💰 Creating sample bids...');
  const bids: Bid[] = [
    {
      id: createId(),
      investorId: investorUser.id,
      invoiceId: invoices[0].id,
      bidAmount: 485000,
      expectedReturn: 515000,
      status: 'PLACED',
      createdAt: new Date(),
    }
  ];

  await db.bid.createMany({ data: bids });

  console.log(`
✅ Seed completed successfully!
Created: 2 Users, 1 Invoice, 1 Bid.
  `);
}

if (require.main === module) {
  main()
    .catch((e) => {
      console.error('❌ Seed failed:', e);
      process.exit(1);
    })
    .finally(async () => {
      await db.$disconnect();
    });
}

export { main as seedScript };