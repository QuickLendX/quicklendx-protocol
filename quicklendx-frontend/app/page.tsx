import Image from "next/image";

export default function Home() {
  return (
    <div className="min-h-screen bg-gradient-to-br from-blue-50 to-indigo-100 flex items-center justify-center p-4">
      <div className="max-w-4xl w-full text-center">
        <div className="mb-8 flex justify-center">
          <div className="relative w-32 h-32">
            <Image
              src="/quicklendx.png"
              alt="QuickLendX Logo"
              fill
              className="object-contain"
              priority
            />
          </div>
        </div>
        
        <h1 className="text-5xl md:text-6xl font-bold text-gray-900 mb-4">
          QuickLendX
        </h1>
        
        <p className="text-xl md:text-2xl text-gray-700 mb-2">
          Decentralized Invoice Financing Platform
        </p>
        
        <p className="text-lg text-gray-600 mb-8 max-w-2xl mx-auto">
          Built on Stellar&apos;s Soroban smart contract platform. 
          Connect businesses with investors through transparent, secure invoice financing.
        </p>
        
        <div className="mt-12 p-6 bg-white/80 backdrop-blur-sm rounded-lg shadow-lg max-w-2xl mx-auto">
          <h2 className="text-2xl font-semibold text-gray-800 mb-4">
            Platform Features
          </h2>
          <div className="grid md:grid-cols-2 gap-4 text-left">
            <div className="p-4 bg-blue-50 rounded-lg">
              <h3 className="font-semibold text-gray-800 mb-2">For Businesses</h3>
              <ul className="text-sm text-gray-600 space-y-1">
                <li>• Upload and manage invoices</li>
                <li>• Receive immediate funding</li>
                <li>• Track invoice status</li>
              </ul>
            </div>
            <div className="p-4 bg-indigo-50 rounded-lg">
              <h3 className="font-semibold text-gray-800 mb-2">For Investors</h3>
              <ul className="text-sm text-gray-600 space-y-1">
                <li>• Browse available invoices</li>
                <li>• Place competitive bids</li>
                <li>• Track investments and returns</li>
              </ul>
            </div>
          </div>
        </div>
        
        <div className="mt-8 text-sm text-gray-500">
          <p>Frontend application is under active development</p>
          <p className="mt-2">
            Smart contracts are deployed and ready for integration
          </p>
        </div>
      </div>
    </div>
  );
}
