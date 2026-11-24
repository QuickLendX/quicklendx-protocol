export default function Loading() {
  return (
    <div
      role="status"
      aria-busy="true"
      aria-label="Loading investor marketplace"
      className="max-w-7xl mx-auto px-4 py-8 sm:px-6 lg:px-8"
    >
      <div
        aria-hidden="true"
        className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 animate-pulse"
      >
        {Array.from({ length: 6 }, (_, index) => (
          <div
            key={index}
            className="bg-white rounded-xl border border-gray-200 p-6 h-64"
          >
            <div className="h-4 bg-gray-200 rounded w-1/3 mb-4" />
            <div className="h-8 bg-gray-200 rounded w-2/3 mb-4" />
            <div className="h-4 bg-gray-200 rounded w-1/2 mb-2" />
            <div className="h-4 bg-gray-200 rounded w-full mt-6" />
          </div>
        ))}
      </div>
    </div>
  );
}
