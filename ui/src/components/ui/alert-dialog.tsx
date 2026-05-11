"use client"

import * as React from "react"
import * as AlertDialogPrimitive from "@radix-ui/react-alert-dialog"
import { motion, AnimatePresence } from "motion/react"

import { cn } from "@/lib/utils"
import { buttonVariants } from "@/components/ui/button"

/**
 * Shared open-state context for the motion-orchestrated AlertDialog.
 *
 * Why a custom context: Radix's `Root` controls mount/unmount of its
 * Content via the `open` prop. With pure CSS animations, that's fine,
 * but the unmount happens as soon as `open` flips false — Radix waits
 * for `animationend`, which gets cut short under some conditions and
 * makes the exit animation feel abrupt.
 *
 * The motion approach: read `open` from context, gate AnimatePresence
 * on it, and use `forceMount` on Radix primitives so they stay in the
 * DOM until motion's exit animation completes. Keeps the existing
 * `<AlertDialog open={..} onOpenChange={..}>` callsite API.
 */
const AlertDialogOpenContext = React.createContext<boolean>(false)

interface AlertDialogProps extends React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Root> {}

function AlertDialog({ open, onOpenChange, children, ...rest }: AlertDialogProps) {
  return (
    <AlertDialogOpenContext.Provider value={open ?? false}>
      <AlertDialogPrimitive.Root open={open} onOpenChange={onOpenChange} {...rest}>
        {children}
      </AlertDialogPrimitive.Root>
    </AlertDialogOpenContext.Provider>
  )
}

const AlertDialogTrigger = AlertDialogPrimitive.Trigger

const AlertDialogPortal = AlertDialogPrimitive.Portal

// Tuned smooth-out curve — Apple-flavored ease for modals. Identical
// curve on entry and exit so the round-trip feels symmetric.
const DIALOG_EASE: [number, number, number, number] = [0.32, 0.72, 0, 1]

const AlertDialogContent = React.forwardRef<
  HTMLDivElement,
  Omit<React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Content>, "asChild"> & {
    /** Optional `className` is applied to the motion.div wrapper. */
    className?: string
  }
>(({ className, children, ...props }, ref) => {
  const open = React.useContext(AlertDialogOpenContext)
  return (
    <AnimatePresence>
      {open && (
        <AlertDialogPrimitive.Portal forceMount>
          {/* Overlay — Radix renders the outer fixed-position wrapper;
              motion.div inside drives the opacity fade. Avoiding
              `asChild` on Radix primitives sidesteps a known
              React.Children.only incompatibility with motion.div when
              `forceMount` is also set. */}
          <AlertDialogPrimitive.Overlay
            forceMount
            className="fixed inset-0 z-[100] titlebar-no-drag"
          >
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.22, ease: DIALOG_EASE }}
              className="absolute inset-0 bg-black/25 backdrop-blur-[1px]"
            />
          </AlertDialogPrimitive.Overlay>
          {/* Content — Radix handles outer positioning + focus mgmt;
              motion.div handles the visible card + animation. */}
          <AlertDialogPrimitive.Content
            ref={ref}
            forceMount
            className="fixed left-[50%] top-[50%] z-[100] w-full max-w-lg translate-x-[-50%] translate-y-[-50%] titlebar-no-drag"
            {...props}
          >
            <motion.div
              initial={{ opacity: 0, scale: 0.992 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.992 }}
              transition={{ duration: 0.22, ease: DIALOG_EASE }}
              className={cn(
                "grid gap-4 border bg-background p-6 shadow-lg sm:rounded-lg",
                className,
              )}
            >
              {children}
            </motion.div>
          </AlertDialogPrimitive.Content>
        </AlertDialogPrimitive.Portal>
      )}
    </AnimatePresence>
  )
})
AlertDialogContent.displayName = "AlertDialogContent"

// The old AlertDialogOverlay export was rarely used directly (Content
// already renders the overlay internally). Re-export the Radix primitive
// for any holdout callsite — they get the Radix default behavior, which
// is still good enough for non-orchestrated cases.
const AlertDialogOverlay = AlertDialogPrimitive.Overlay

const AlertDialogHeader = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex flex-col space-y-2 text-center sm:text-left",
      className
    )}
    {...props}
  />
)
AlertDialogHeader.displayName = "AlertDialogHeader"

const AlertDialogFooter = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2",
      className
    )}
    {...props}
  />
)
AlertDialogFooter.displayName = "AlertDialogFooter"

const AlertDialogTitle = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Title
    ref={ref}
    className={cn("text-lg font-semibold", className)}
    {...props}
  />
))
AlertDialogTitle.displayName = AlertDialogPrimitive.Title.displayName

const AlertDialogDescription = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Description
    ref={ref}
    className={cn("text-sm text-muted-foreground", className)}
    {...props}
  />
))
AlertDialogDescription.displayName =
  AlertDialogPrimitive.Description.displayName

const AlertDialogAction = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Action>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Action>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Action
    ref={ref}
    className={cn(buttonVariants(), className)}
    {...props}
  />
))
AlertDialogAction.displayName = AlertDialogPrimitive.Action.displayName

const AlertDialogCancel = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Cancel>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Cancel>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Cancel
    ref={ref}
    className={cn(
      buttonVariants({ variant: "outline" }),
      "mt-2 sm:mt-0",
      className
    )}
    {...props}
  />
))
AlertDialogCancel.displayName = AlertDialogPrimitive.Cancel.displayName

export {
  AlertDialog,
  AlertDialogPortal,
  AlertDialogOverlay,
  AlertDialogTrigger,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogFooter,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogAction,
  AlertDialogCancel,
}
