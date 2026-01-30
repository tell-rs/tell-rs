/// Standard event names from the Tell specification.
///
/// Use these constants with `client.track()` for consistent event naming.
/// Custom string names are always allowed too.
pub struct Events;

impl Events {
    // --- User Lifecycle ---
    pub const USER_SIGNED_UP: &str = "User Signed Up";
    pub const USER_SIGNED_IN: &str = "User Signed In";
    pub const USER_SIGNED_OUT: &str = "User Signed Out";
    pub const USER_INVITED: &str = "User Invited";
    pub const USER_ONBOARDED: &str = "User Onboarded";
    pub const AUTHENTICATION_FAILED: &str = "Authentication Failed";
    pub const PASSWORD_RESET: &str = "Password Reset";
    pub const TWO_FACTOR_ENABLED: &str = "Two Factor Enabled";
    pub const TWO_FACTOR_DISABLED: &str = "Two Factor Disabled";

    // --- Revenue & Billing ---
    pub const ORDER_COMPLETED: &str = "Order Completed";
    pub const ORDER_REFUNDED: &str = "Order Refunded";
    pub const ORDER_CANCELED: &str = "Order Canceled";
    pub const PAYMENT_FAILED: &str = "Payment Failed";
    pub const PAYMENT_METHOD_ADDED: &str = "Payment Method Added";
    pub const PAYMENT_METHOD_UPDATED: &str = "Payment Method Updated";
    pub const PAYMENT_METHOD_REMOVED: &str = "Payment Method Removed";

    // --- Subscription ---
    pub const SUBSCRIPTION_STARTED: &str = "Subscription Started";
    pub const SUBSCRIPTION_RENEWED: &str = "Subscription Renewed";
    pub const SUBSCRIPTION_PAUSED: &str = "Subscription Paused";
    pub const SUBSCRIPTION_RESUMED: &str = "Subscription Resumed";
    pub const SUBSCRIPTION_CHANGED: &str = "Subscription Changed";
    pub const SUBSCRIPTION_CANCELED: &str = "Subscription Canceled";

    // --- Trial ---
    pub const TRIAL_STARTED: &str = "Trial Started";
    pub const TRIAL_ENDING_SOON: &str = "Trial Ending Soon";
    pub const TRIAL_ENDED: &str = "Trial Ended";
    pub const TRIAL_CONVERTED: &str = "Trial Converted";

    // --- Shopping ---
    pub const CART_VIEWED: &str = "Cart Viewed";
    pub const CART_UPDATED: &str = "Cart Updated";
    pub const CART_ABANDONED: &str = "Cart Abandoned";
    pub const CHECKOUT_STARTED: &str = "Checkout Started";
    pub const CHECKOUT_COMPLETED: &str = "Checkout Completed";

    // --- Engagement ---
    pub const PAGE_VIEWED: &str = "Page Viewed";
    pub const FEATURE_USED: &str = "Feature Used";
    pub const SEARCH_PERFORMED: &str = "Search Performed";
    pub const FILE_UPLOADED: &str = "File Uploaded";
    pub const NOTIFICATION_SENT: &str = "Notification Sent";
    pub const NOTIFICATION_CLICKED: &str = "Notification Clicked";

    // --- Communication ---
    pub const EMAIL_SENT: &str = "Email Sent";
    pub const EMAIL_OPENED: &str = "Email Opened";
    pub const EMAIL_CLICKED: &str = "Email Clicked";
    pub const EMAIL_BOUNCED: &str = "Email Bounced";
    pub const EMAIL_UNSUBSCRIBED: &str = "Email Unsubscribed";
    pub const SUPPORT_TICKET_CREATED: &str = "Support Ticket Created";
    pub const SUPPORT_TICKET_RESOLVED: &str = "Support Ticket Resolved";
}
